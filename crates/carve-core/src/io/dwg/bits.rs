//! The bit level codes DWG data is written in.
//!
//! Most DWG data is a stream of bits read from the high bit of each byte
//! down, holding numbers in short forms that spend two bits on saying how
//! the value is stored (a byte, a whole short, or a common value such as 0
//! or 1.0 given by the two bits alone). Every read here is checked against
//! the end of the stream: running past it is an error, never a panic.

use super::{Bad, Version};

/// A bit cursor over a byte slice.
#[derive(Clone)]
pub(super) struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits from the start of `data`.
    pos: usize,
    /// First bit past the readable range.
    end: usize,
    pub(super) version: Version,
}

/// A handle reference as stored: a code and a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HandleRef {
    pub(super) code: u8,
    pub(super) value: u64,
}

impl HandleRef {
    /// The handle this reference points at, taking relative codes from
    /// `own`, the handle of the object that holds the reference.
    pub(super) fn resolve(self, own: u64) -> u64 {
        match self.code {
            0x6 => own.wrapping_add(1),
            0x8 => own.wrapping_sub(1),
            0xA => own.wrapping_add(self.value),
            0xC => own.wrapping_sub(self.value),
            _ => self.value,
        }
    }
}

impl<'a> Bits<'a> {
    pub(super) fn new(data: &'a [u8], version: Version) -> Self {
        Bits {
            data,
            pos: 0,
            end: data.len().saturating_mul(8),
            version,
        }
    }

    /// A reader over `data[start..start + len]` (bytes), clamped to the data.
    pub(super) fn slice(data: &'a [u8], start: usize, len: usize, version: Version) -> Self {
        let start = start.min(data.len());
        let stop = start.saturating_add(len).min(data.len());
        Bits::new(&data[start..stop], version)
    }

    pub(super) fn position(&self) -> usize {
        self.pos
    }

    pub(super) fn end(&self) -> usize {
        self.end
    }

    /// Bits left before the end.
    pub(super) fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Moves to bit `pos`.
    pub(super) fn seek(&mut self, pos: usize) -> Result<(), Bad> {
        if pos > self.data.len().saturating_mul(8) {
            return Err(Bad("a position past the end of the data"));
        }
        self.pos = pos;
        Ok(())
    }

    /// Stops reads at bit `end` from now on.
    pub(super) fn limit(&mut self, end: usize) {
        self.end = end.min(self.data.len().saturating_mul(8));
    }

    /// Skips `n` bits.
    pub(super) fn skip(&mut self, n: usize) -> Result<(), Bad> {
        if n > self.remaining() {
            return Err(Bad("data ran past its end"));
        }
        self.pos += n;
        Ok(())
    }

    pub(super) fn b(&mut self) -> Result<bool, Bad> {
        if self.pos >= self.end {
            return Err(Bad("data ran past its end"));
        }
        let byte = self.data[self.pos >> 3];
        let bit = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Ok(bit == 1)
    }

    /// Up to 64 bits, first bit highest.
    fn bits(&mut self, n: usize) -> Result<u64, Bad> {
        if n > self.remaining() {
            return Err(Bad("data ran past its end"));
        }
        let mut value = 0u64;
        let mut left = n;
        while left > 0 {
            let offset = self.pos & 7;
            let take = (8 - offset).min(left);
            let byte = u64::from(self.data[self.pos >> 3]);
            let chunk = (byte >> (8 - offset - take)) & ((1 << take) - 1);
            value = (value << take) | chunk;
            self.pos += take;
            left -= take;
        }
        Ok(value)
    }

    pub(super) fn bb(&mut self) -> Result<u8, Bad> {
        Ok(self.bits(2)? as u8)
    }

    pub(super) fn rc(&mut self) -> Result<u8, Bad> {
        Ok(self.bits(8)? as u8)
    }

    pub(super) fn rs(&mut self) -> Result<u16, Bad> {
        let lo = u16::from(self.rc()?);
        let hi = u16::from(self.rc()?);
        Ok(lo | (hi << 8))
    }

    pub(super) fn rl(&mut self) -> Result<u32, Bad> {
        let lo = u32::from(self.rs()?);
        let hi = u32::from(self.rs()?);
        Ok(lo | (hi << 16))
    }

    pub(super) fn rd(&mut self) -> Result<f64, Bad> {
        let mut bytes = [0u8; 8];
        for byte in &mut bytes {
            *byte = self.rc()?;
        }
        Ok(f64::from_le_bytes(bytes))
    }

    pub(super) fn bs(&mut self) -> Result<i16, Bad> {
        Ok(match self.bb()? {
            0 => self.rs()? as i16,
            1 => i16::from(self.rc()?),
            2 => 0,
            _ => 256,
        })
    }

    pub(super) fn bl(&mut self) -> Result<i32, Bad> {
        Ok(match self.bb()? {
            0 => self.rl()? as i32,
            1 => i32::from(self.rc()?),
            2 => 0,
            _ => return Err(Bad("an unused long code")),
        })
    }

    /// A count read as a long, refused when negative or when fewer than
    /// `bits_each` bits per item are left, so a damaged count cannot ask
    /// for a huge allocation.
    pub(super) fn count(&mut self, bits_each: usize) -> Result<usize, Bad> {
        let n = self.bl()?;
        self.check_count(n, bits_each)
    }

    pub(super) fn check_count(&self, n: i32, bits_each: usize) -> Result<usize, Bad> {
        let n = usize::try_from(n).map_err(|_| Bad("a negative count"))?;
        if n.saturating_mul(bits_each.max(1)) > self.remaining() {
            return Err(Bad("a count larger than its data"));
        }
        Ok(n)
    }

    /// A long long: a three bit byte count, then the bytes, low first.
    pub(super) fn bll(&mut self) -> Result<u64, Bad> {
        let len = self.bits(3)? as u8;
        let mut value = 0u64;
        for i in 0..len {
            value |= u64::from(self.rc()?) << (8 * u32::from(i));
        }
        Ok(value)
    }

    pub(super) fn bd(&mut self) -> Result<f64, Bad> {
        Ok(match self.bb()? {
            0 => self.rd()?,
            1 => 1.0,
            2 => 0.0,
            _ => return Err(Bad("an unused double code")),
        })
    }

    /// A double stored as a change from `default`.
    pub(super) fn dd(&mut self, default: f64) -> Result<f64, Bad> {
        let mut bytes = default.to_le_bytes();
        match self.bb()? {
            0 => {}
            1 => {
                for byte in &mut bytes[..4] {
                    *byte = self.rc()?;
                }
            }
            2 => {
                bytes[4] = self.rc()?;
                bytes[5] = self.rc()?;
                for byte in &mut bytes[..4] {
                    *byte = self.rc()?;
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(bytes))
    }

    pub(super) fn bd3(&mut self) -> Result<[f64; 3], Bad> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    pub(super) fn rd2(&mut self) -> Result<[f64; 2], Bad> {
        Ok([self.rd()?, self.rd()?])
    }

    /// An extrusion: from R2000 one bit says it is the default (0, 0, 1).
    pub(super) fn be(&mut self) -> Result<[f64; 3], Bad> {
        if self.version >= Version::R2000 && self.b()? {
            return Ok([0.0, 0.0, 1.0]);
        }
        self.bd3()
    }

    /// A thickness: from R2000 one bit says it is 0.
    pub(super) fn bt(&mut self) -> Result<f64, Bad> {
        if self.version >= Version::R2000 && self.b()? {
            return Ok(0.0);
        }
        self.bd()
    }

    /// A modular char: 7 bits a byte, low byte first, the high bit saying
    /// another byte follows; the last byte's 0x40 bit makes it negative.
    pub(super) fn mc(&mut self) -> Result<i64, Bad> {
        let mut value = 0u64;
        for i in 0..9 {
            let byte = self.rc()?;
            if byte & 0x80 == 0 {
                let negative = byte & 0x40 != 0;
                value |= u64::from(byte & 0x3F) << (7 * i);
                let value = value as i64;
                return Ok(if negative { -value } else { value });
            }
            value |= u64::from(byte & 0x7F) << (7 * i);
        }
        Err(Bad("a modular number too long"))
    }

    /// A modular char with no sign bit.
    pub(super) fn umc(&mut self) -> Result<u64, Bad> {
        let mut value = 0u64;
        for i in 0..9 {
            let byte = self.rc()?;
            value |= u64::from(byte & 0x7F) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Bad("a modular number too long"))
    }

    /// A modular short: 15 bits a short, the high bit saying another follows.
    pub(super) fn ms(&mut self) -> Result<u64, Bad> {
        let mut value = 0u64;
        for i in 0..4 {
            let word = self.rs()?;
            value |= u64::from(word & 0x7FFF) << (15 * i);
            if word & 0x8000 == 0 {
                return Ok(value);
            }
        }
        Err(Bad("a modular number too long"))
    }

    pub(super) fn h(&mut self) -> Result<HandleRef, Bad> {
        let first = self.rc()?;
        let code = first >> 4;
        let len = first & 0x0F;
        if len > 8 {
            return Err(Bad("a handle longer than 8 bytes"));
        }
        let mut value = 0u64;
        for _ in 0..len {
            value = (value << 8) | u64::from(self.rc()?);
        }
        Ok(HandleRef { code, value })
    }

    /// Text of 8 bit characters (R2004 and before).
    pub(super) fn t(&mut self) -> Result<String, Bad> {
        let len = usize::from(self.bs()? as u16);
        if len.saturating_mul(8) > self.remaining() {
            return Err(Bad("a string longer than its data"));
        }
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(self.rc()?);
        }
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        // Names are mostly ASCII; other bytes are taken as Latin 1.
        Ok(match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
        })
    }

    /// Text of UTF-16 characters (R2007 on).
    pub(super) fn tu(&mut self) -> Result<String, Bad> {
        let len = usize::from(self.bs()? as u16);
        if len.saturating_mul(16) > self.remaining() {
            return Err(Bad("a string longer than its data"));
        }
        let mut units = Vec::with_capacity(len);
        for _ in 0..len {
            units.push(self.rs()?);
        }
        while units.last() == Some(&0) {
            units.pop();
        }
        Ok(String::from_utf16_lossy(&units))
    }

    /// Text in this release's form.
    pub(super) fn tv(&mut self) -> Result<String, Bad> {
        if self.version >= Version::R2007 {
            self.tu()
        } else {
            self.t()
        }
    }

    /// An object type: a bit short, or from R2010 a pair of bits and one
    /// or two bytes.
    pub(super) fn ot(&mut self) -> Result<u16, Bad> {
        if self.version < Version::R2010 {
            return Ok(self.bs()? as u16);
        }
        Ok(match self.bb()? {
            0 => u16::from(self.rc()?),
            1 => u16::from(self.rc()?) + 0x1F0,
            _ => self.rs()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes from a string of 0 and 1 characters, padded with zeros.
    fn from_bits(text: &str) -> Vec<u8> {
        let bits: Vec<u8> = text
            .bytes()
            .filter(|c| *c == b'0' || *c == b'1')
            .map(|c| c - b'0')
            .collect();
        bits.chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, bit)| acc | (bit << (7 - i)))
            })
            .collect()
    }

    #[test]
    fn short_forms_read_as_written() {
        let data = from_bits("00 00000001 00000001 10 11 01 00001111 10");
        let mut bits = Bits::new(&data, Version::R2000);
        let values: Vec<i16> = (0..5).map(|_| bits.bs().expect("short")).collect();
        assert_eq!(values, vec![257, 0, 256, 15, 0]);
        let data = from_bits("00 00000001 00000001 00000000 00000000 10 01 00001111 10");
        let mut bits = Bits::new(&data, Version::R2000);
        let values: Vec<i32> = (0..4).map(|_| bits.bl().expect("long")).collect();
        assert_eq!(values, vec![257, 0, 15, 0]);
        let data = from_bits("01 10");
        let mut bits = Bits::new(&data, Version::R2000);
        assert_eq!(bits.bd().expect("one"), 1.0);
        assert_eq!(bits.bd().expect("zero"), 0.0);
    }

    #[test]
    fn modular_numbers() {
        let data = [0b1000_0010, 0b0010_0100];
        assert_eq!(Bits::new(&data, Version::R2000).mc().expect("mc"), 4610);
        let data = [0b1110_1001, 0b1001_0111, 0b1110_0110, 0b0011_0101];
        assert_eq!(
            Bits::new(&data, Version::R2000).mc().expect("mc"),
            112_823_273
        );
        let data = [0b1000_0101, 0b0100_1011];
        assert_eq!(Bits::new(&data, Version::R2000).mc().expect("mc"), -1413);
        let data = [0b0011_0001, 0b1111_0100, 0b1000_1101, 0b0000_0000];
        assert_eq!(
            Bits::new(&data, Version::R2000).ms().expect("ms"),
            4_650_033
        );
    }

    #[test]
    fn handles_and_defaults() {
        let data = [0b0101_0010, 0b0000_0101, 0b1110_0111];
        let h = Bits::new(&data, Version::R2000).h().expect("handle");
        assert_eq!(
            h,
            HandleRef {
                code: 5,
                value: 0x5E7
            }
        );
        assert_eq!(HandleRef { code: 6, value: 0 }.resolve(9), 10);
        assert_eq!(
            HandleRef {
                code: 0xC,
                value: 2
            }
            .resolve(9),
            7
        );
        // Default double: 00 keeps the default.
        let data = [0u8];
        assert_eq!(Bits::new(&data, Version::R2000).dd(2.5).expect("dd"), 2.5);
    }

    #[test]
    fn reads_past_the_end_are_errors() {
        let data = [0xFFu8];
        let mut bits = Bits::new(&data, Version::R2000);
        assert!(bits.rs().is_err());
        let mut bits = Bits::new(&data, Version::R2000);
        assert!(bits.bl().is_err());
        let data = [0b0000_0000, 0xFF, 0xFF, 0xFF];
        let mut bits = Bits::new(&data, Version::R2000);
        assert!(bits.count(8).is_err());
    }
}
