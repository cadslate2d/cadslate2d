//! The bit codes of ODA spec §2: a reader over a byte slice, most
//! significant bit first, that reads the compressed forms every section
//! and object is written in.
//!
//! Where the specification is wrong, the code follows the files and the
//! ADR-0036 errata list in `docs/dwg-matrix.md`.

use cad_core::Point2;

use crate::{DwgError, ImportLimits, Version};

/// A handle reference (§2.13): the code says what kind of reference it is,
/// the value is the handle itself or, for codes 6 to 0xC, an offset the
/// caller resolves against a reference handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Handle {
    pub code: u8,
    pub value: u64,
}

impl Handle {
    /// The handle this reference names, given the handle it is relative to
    /// (§2.13): codes 2 to 5 are absolute, 6 and 8 the next and previous,
    /// 0xA and 0xC an offset forwards and backwards.
    pub fn resolve(self, reference: u64) -> u64 {
        match self.code {
            0x6 => reference.wrapping_add(1),
            0x8 => reference.wrapping_sub(1),
            0xA => reference.wrapping_add(self.value),
            0xC => reference.wrapping_sub(self.value),
            _ => self.value,
        }
    }

    pub fn is_null(self) -> bool {
        self.value == 0 && !matches!(self.code, 0x6 | 0x8)
    }
}

/// A colour as an entity or a table row stores it (§2.11): an index into
/// the AutoCAD colour table, with R2004's additions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    /// The colour index: 0 ByBlock, 256 ByLayer, 1 to 255 a colour.
    pub index: i16,
    /// A true colour as `0x00RRGGBB`, when the file carries one.
    pub rgb: Option<u32>,
    /// A colour book colour: its handle is in the handle stream.
    pub has_book_handle: bool,
    /// Transparency, as the file stores it: the low byte is the value,
    /// the high byte the kind (0 by layer, 1 by block, 3 a value).
    pub transparency: Option<u32>,
}

/// A reader of bit codes over a slice, positioned in bits.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            end: data.len() * 8,
        }
    }

    /// A reader over the bits `start..end` of the same slice.
    pub fn window(&self, start: usize, end: usize) -> Result<Self, DwgError> {
        if start > end || end > self.data.len() * 8 {
            return Err(self.corrupt("a stream bound"));
        }
        Ok(Self {
            data: self.data,
            pos: start,
            end,
        })
    }

    /// The position in bits from the start of the slice.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The position in whole bytes, rounded down.
    pub fn byte_position(&self) -> usize {
        self.pos / 8
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn set_position(&mut self, bits: usize) -> Result<(), DwgError> {
        if bits > self.end {
            return Err(self.truncated("a stream position"));
        }
        self.pos = bits;
        Ok(())
    }

    pub fn remaining_bits(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Skips to the next byte boundary.
    pub fn align(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
    }

    fn truncated(&self, what: &'static str) -> DwgError {
        DwgError::Truncated {
            what,
            at: self.pos / 8,
        }
    }

    fn corrupt(&self, what: &'static str) -> DwgError {
        DwgError::Corrupt {
            what,
            at: self.pos / 8,
        }
    }

    /// B: one bit.
    pub fn bit(&mut self) -> Result<u8, DwgError> {
        if self.pos >= self.end {
            return Err(self.truncated("a bit stream"));
        }
        let b = (self.data[self.pos / 8] >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Ok(b)
    }

    /// Up to 64 bits as an unsigned number, most significant first.
    pub fn bits(&mut self, n: u32) -> Result<u64, DwgError> {
        debug_assert!(n <= 64);
        if self.pos + n as usize > self.end {
            return Err(self.truncated("a bit stream"));
        }
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | u64::from(self.bit()?);
        }
        Ok(v)
    }

    /// BB: two bits.
    pub fn bb(&mut self) -> Result<u8, DwgError> {
        Ok(self.bits(2)? as u8)
    }

    /// 3B: one to three bits, up to the first zero bit (§2.1).
    pub fn b3(&mut self) -> Result<u8, DwgError> {
        let mut v = 0u8;
        for _ in 0..3 {
            let b = self.bit()?;
            v = (v << 1) | b;
            if b == 0 {
                break;
            }
        }
        Ok(v)
    }

    /// RC: a raw byte.
    pub fn rc(&mut self) -> Result<u8, DwgError> {
        if self.pos.is_multiple_of(8) {
            if self.pos + 8 > self.end {
                return Err(self.truncated("a byte"));
            }
            let b = self.data[self.pos / 8];
            self.pos += 8;
            return Ok(b);
        }
        Ok(self.bits(8)? as u8)
    }

    /// RS: a raw short, least significant byte first.
    pub fn rs(&mut self) -> Result<u16, DwgError> {
        let lo = u16::from(self.rc()?);
        let hi = u16::from(self.rc()?);
        Ok(lo | (hi << 8))
    }

    /// RL: a raw long, least significant byte first.
    pub fn rl(&mut self) -> Result<u32, DwgError> {
        let lo = u32::from(self.rs()?);
        let hi = u32::from(self.rs()?);
        Ok(lo | (hi << 16))
    }

    /// RLL: eight raw bytes, least significant first.
    pub fn rll(&mut self) -> Result<u64, DwgError> {
        let lo = u64::from(self.rl()?);
        let hi = u64::from(self.rl()?);
        Ok(lo | (hi << 32))
    }

    /// RD: a raw IEEE double.
    pub fn rd(&mut self) -> Result<f64, DwgError> {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.rc()?;
        }
        Ok(f64::from_le_bytes(b))
    }

    /// 2RD: two raw doubles.
    pub fn rd2(&mut self) -> Result<Point2, DwgError> {
        Ok(Point2::new(self.rd()?, self.rd()?))
    }

    /// BS: a bit short (§2.2).
    pub fn bs(&mut self) -> Result<i16, DwgError> {
        Ok(match self.bb()? {
            0 => self.rs()? as i16,
            1 => i16::from(self.rc()?),
            2 => 0,
            _ => 256,
        })
    }

    /// BL: a bit long (§2.3).
    pub fn bl(&mut self) -> Result<i32, DwgError> {
        match self.bb()? {
            0 => Ok(self.rl()? as i32),
            1 => Ok(i32::from(self.rc()?)),
            2 => Ok(0),
            _ => Err(self.corrupt("a bit long")),
        }
    }

    /// BLL: a bit long long (§2.4).
    pub fn bll(&mut self) -> Result<u64, DwgError> {
        let n = self.b3()?;
        let mut v = 0u64;
        for i in 0..n {
            v |= u64::from(self.rc()?) << (8 * i);
        }
        Ok(v)
    }

    /// BD: a bit double (§2.5).
    pub fn bd(&mut self) -> Result<f64, DwgError> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(self.corrupt("a bit double")),
        }
    }

    /// 2BD and 3BD: points of bit doubles. The z of a 3D point is read and
    /// returned beside the plane point.
    pub fn bd2(&mut self) -> Result<Point2, DwgError> {
        Ok(Point2::new(self.bd()?, self.bd()?))
    }

    pub fn bd3(&mut self) -> Result<(Point2, f64), DwgError> {
        let p = self.bd2()?;
        Ok((p, self.bd()?))
    }

    /// DD: a bit double with a default (§2.9).
    pub fn dd(&mut self, default: f64) -> Result<f64, DwgError> {
        match self.bb()? {
            0 => Ok(default),
            1 => {
                let mut b = default.to_le_bytes();
                for x in &mut b[..4] {
                    *x = self.rc()?;
                }
                Ok(f64::from_le_bytes(b))
            }
            2 => {
                let mut b = default.to_le_bytes();
                b[4] = self.rc()?;
                b[5] = self.rc()?;
                for x in &mut b[..4] {
                    *x = self.rc()?;
                }
                Ok(f64::from_le_bytes(b))
            }
            _ => self.rd(),
        }
    }

    /// BT: a thickness (§2.10): one bit says it is zero, else a BD.
    pub fn bt(&mut self) -> Result<f64, DwgError> {
        if self.bit()? == 1 { Ok(0.0) } else { self.bd() }
    }

    /// BE: an extrusion (§2.8): one bit says it is (0, 0, 1), else 3BD.
    pub fn be(&mut self) -> Result<[f64; 3], DwgError> {
        if self.bit()? == 1 {
            Ok([0.0, 0.0, 1.0])
        } else {
            Ok([self.bd()?, self.bd()?, self.bd()?])
        }
    }

    /// MC: a modular char (§2.6), signed by the 0x40 bit of the last byte.
    pub fn mc(&mut self) -> Result<i64, DwgError> {
        let mut v: i64 = 0;
        let mut shift = 0;
        loop {
            let b = self.rc()?;
            if b & 0x80 == 0 {
                v |= i64::from(b & 0x3f) << shift;
                return Ok(if b & 0x40 != 0 { -v } else { v });
            }
            v |= i64::from(b & 0x7f) << shift;
            shift += 7;
            if shift > 56 {
                return Err(self.corrupt("a modular char"));
            }
        }
    }

    /// A modular char without the sign bit: how the object map stores
    /// handle offsets, which only ever grow (§2.6).
    pub fn umc(&mut self) -> Result<u64, DwgError> {
        let mut v: u64 = 0;
        let mut shift = 0;
        loop {
            let b = self.rc()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 56 {
                return Err(self.corrupt("a modular char"));
            }
        }
    }

    /// MS: a modular short (§2.7).
    pub fn ms(&mut self) -> Result<u32, DwgError> {
        let mut v: u32 = 0;
        let mut shift = 0;
        loop {
            let s = self.rs()?;
            v |= u32::from(s & 0x7fff) << shift;
            if s & 0x8000 == 0 {
                return Ok(v);
            }
            shift += 15;
            if shift > 30 {
                return Err(self.corrupt("a modular short"));
            }
        }
    }

    /// H: a handle reference (§2.13).
    pub fn handle(&mut self) -> Result<Handle, DwgError> {
        let code = self.bits(4)? as u8;
        let n = self.bits(4)? as u8;
        let mut value = 0u64;
        for _ in 0..n {
            value = (value << 8) | u64::from(self.rc()?);
        }
        Ok(Handle { code, value })
    }

    /// T: a text of the R2000 and R2004 kind, a bit short length and that
    /// many bytes in the drawing's code page. A terminating NUL inside the
    /// count is dropped.
    pub fn text(&mut self, limits: &ImportLimits) -> Result<Vec<u8>, DwgError> {
        let n = self.bs()?;
        let n = usize::try_from(n).map_err(|_| self.corrupt("a text length"))?;
        if n > limits.max_string_bytes {
            return Err(DwgError::TooMany {
                what: "bytes in a string",
                limit: limits.max_string_bytes,
            });
        }
        if self.pos + n * 8 > self.end {
            return Err(self.truncated("a string"));
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.rc()?);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        Ok(out)
    }

    /// TU: a Unicode text of the R2007 kind, a bit short count of UTF-16
    /// units.
    pub fn text_unicode(&mut self, limits: &ImportLimits) -> Result<String, DwgError> {
        let n = self.bs()?;
        let n = usize::try_from(n).map_err(|_| self.corrupt("a text length"))?;
        if n * 2 > limits.max_string_bytes {
            return Err(DwgError::TooMany {
                what: "bytes in a string",
                limit: limits.max_string_bytes,
            });
        }
        if self.pos + n * 16 > self.end {
            return Err(self.truncated("a string"));
        }
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            units.push(self.rs()?);
        }
        while units.last() == Some(&0) {
            units.pop();
        }
        Ok(String::from_utf16_lossy(&units))
    }

    /// CMC: a colour as a table row or the header stores it (§2.11). From
    /// R2004 the long carries the colour with a method in its high byte:
    /// 0xC0 by layer, 0xC1 by block, 0xC2 a true colour in the low three
    /// bytes, 0xC3 an index in the low byte; the specification does not
    /// say so (ADR-0036 errata).
    pub fn cmc(&mut self, version: Version, limits: &ImportLimits) -> Result<Color, DwgError> {
        let index = self.bs()?;
        let mut color = Color {
            index,
            ..Color::default()
        };
        if version >= Version::R2004 {
            let value = self.bl()? as u32;
            let byte = self.rc()?;
            match value >> 24 {
                0xC0 => color.index = 256,
                0xC1 => color.index = 0,
                0xC2 => color.rgb = Some(value & 0x00ff_ffff),
                0xC3 => color.index = (value & 0xff) as i16,
                _ => {}
            }
            // Colour and book names, read and dropped.
            if byte & 1 != 0 {
                self.text(limits)?;
            }
            if byte & 2 != 0 {
                self.text(limits)?;
            }
        }
        Ok(color)
    }

    /// ENC: a colour as an entity stores it (§2.11). In R2004 and later
    /// the high byte of the short carries flags: 0x80 an RGB value follows
    /// (a BL, not the BS the specification says), 0x40 a colour book
    /// handle is in the handle stream, 0x20 a transparency follows.
    pub fn enc(&mut self, version: Version) -> Result<Color, DwgError> {
        let raw = self.bs()? as u16;
        if version < Version::R2004 {
            return Ok(Color {
                index: raw as i16,
                ..Color::default()
            });
        }
        let flags = raw >> 8;
        let mut color = Color {
            index: (raw & 0x1ff) as i16,
            ..Color::default()
        };
        if flags & 0x80 != 0 {
            color.rgb = Some(self.bl()? as u32 & 0x00ff_ffff);
        }
        color.has_book_handle = flags & 0x40 != 0;
        if flags & 0x20 != 0 {
            color.transparency = Some(self.bl()? as u32);
        }
        Ok(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes from a string of bits, the last byte padded with zeros.
    fn bytes(bits: &str) -> Vec<u8> {
        let bits: Vec<u8> = bits.bytes().filter(|b| *b == b'0' || *b == b'1').collect();
        bits.chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (b - b'0') << (7 - i))
                    .sum()
            })
            .collect()
    }

    /// A bit string with raw bytes appended as bits, so that what follows
    /// a code is read where the code leaves off, not at a byte boundary.
    fn then(bits: &str, raw: &[u8]) -> String {
        let mut out = bits.to_string();
        for b in raw {
            out.push_str(&format!("{b:08b}"));
        }
        out
    }

    #[test]
    fn five_bit_shorts_of_the_specifications_example() {
        // §2.2: 257, 0, 256, 15, 0.
        let data = bytes("00 00000001 00000001 10 11 01 00001111 10");
        let mut r = BitReader::new(&data);
        assert_eq!(r.bs().unwrap(), 257);
        assert_eq!(r.bs().unwrap(), 0);
        assert_eq!(r.bs().unwrap(), 256);
        assert_eq!(r.bs().unwrap(), 15);
        assert_eq!(r.bs().unwrap(), 0);
        // A negative short uses the short form.
        let data = bytes("00 11111111 11111111");
        assert_eq!(BitReader::new(&data).bs().unwrap(), -1);
    }

    #[test]
    fn four_bit_longs_of_the_specifications_example() {
        // §2.3: 257, 0, 15, 0.
        let data = bytes("00 00000001 00000001 00000000 00000000 10 01 00001111 10");
        let mut r = BitReader::new(&data);
        assert_eq!(r.bl().unwrap(), 257);
        assert_eq!(r.bl().unwrap(), 0);
        assert_eq!(r.bl().unwrap(), 15);
        assert_eq!(r.bl().unwrap(), 0);
        let data = bytes("11");
        assert!(BitReader::new(&data).bl().is_err());
    }

    #[test]
    fn modular_chars_and_shorts_of_the_specifications_examples() {
        // §2.6
        let mut r = BitReader::new(&[0x82, 0x24]);
        assert_eq!(r.mc().unwrap(), 4610);
        let mut r = BitReader::new(&[0xE9, 0x97, 0xE6, 0x35]);
        assert_eq!(r.mc().unwrap(), 112_823_273);
        let mut r = BitReader::new(&[0x85, 0x4B]);
        assert_eq!(r.mc().unwrap(), -1413);
        // The object map's unsigned form reads the same bytes as 4610.
        let mut r = BitReader::new(&[0x82, 0x24]);
        assert_eq!(r.umc().unwrap(), 4610);
        // §2.7
        let mut r = BitReader::new(&[0x31, 0xF4, 0x8D, 0x00]);
        assert_eq!(r.ms().unwrap(), 4_650_033);
        // A modular char that never ends is corrupt, not a hang.
        let data = [0xff; 12];
        assert!(BitReader::new(&data).mc().is_err());
        assert!(BitReader::new(&data).umc().is_err());
        assert!(BitReader::new(&data).ms().is_err());
    }

    #[test]
    fn a_handle_of_the_specifications_example_and_its_resolution() {
        // §2.13: 5.2.05E7
        let data = bytes("0101 0010 00000101 11100111");
        let h = BitReader::new(&data).handle().unwrap();
        assert_eq!(
            h,
            Handle {
                code: 5,
                value: 0x5E7
            }
        );
        assert_eq!(h.resolve(10), 0x5E7);
        assert_eq!(Handle { code: 6, value: 0 }.resolve(10), 11);
        assert_eq!(Handle { code: 8, value: 0 }.resolve(10), 9);
        assert_eq!(
            Handle {
                code: 0xA,
                value: 5
            }
            .resolve(10),
            15
        );
        assert_eq!(
            Handle {
                code: 0xC,
                value: 5
            }
            .resolve(10),
            5
        );
        assert!(Handle { code: 5, value: 0 }.is_null());
        assert!(!Handle { code: 6, value: 0 }.is_null());
    }

    #[test]
    fn bit_doubles_defaults_thickness_and_extrusion() {
        let data = bytes(
            "01 10 00 00000000 00000000 00000000 00000000 00000000 00000000 11110000 00111111",
        );
        let mut r = BitReader::new(&data);
        assert_eq!(r.bd().unwrap(), 1.0);
        assert_eq!(r.bd().unwrap(), 0.0);
        assert_eq!(r.bd().unwrap(), 1.0);
        // DD: default kept; four bytes patched; six bytes patched; a full double.
        let patched = {
            let mut b = 2.5f64.to_le_bytes();
            b[..4].copy_from_slice(&[1, 2, 3, 4]);
            f64::from_le_bytes(b)
        };
        let data = bytes(&then("00 01", &[1, 2, 3, 4]));
        let mut r = BitReader::new(&data);
        assert_eq!(r.dd(2.5).unwrap(), 2.5);
        assert_eq!(r.dd(2.5).unwrap(), patched);
        let data = bytes(&then("10", &[9, 8, 1, 2, 3, 4]));
        let expected = {
            let mut b = 2.5f64.to_le_bytes();
            b[4] = 9;
            b[5] = 8;
            b[..4].copy_from_slice(&[1, 2, 3, 4]);
            f64::from_le_bytes(b)
        };
        assert_eq!(BitReader::new(&data).dd(2.5).unwrap(), expected);
        let data = bytes(&then("11", &7.25f64.to_le_bytes()));
        assert_eq!(BitReader::new(&data).dd(2.5).unwrap(), 7.25);
        // BT and BE.
        let data = bytes("1 0 10 1 0 01 01 10");
        let mut r = BitReader::new(&data);
        assert_eq!(r.bt().unwrap(), 0.0);
        assert_eq!(r.bt().unwrap(), 0.0);
        assert_eq!(r.be().unwrap(), [0.0, 0.0, 1.0]);
        assert_eq!(r.be().unwrap(), [1.0, 1.0, 0.0]);
    }

    #[test]
    fn three_bit_counts_and_long_longs() {
        let data = bytes("0 10 110 111");
        let mut r = BitReader::new(&data);
        assert_eq!(r.b3().unwrap(), 0);
        assert_eq!(r.b3().unwrap(), 2);
        assert_eq!(r.b3().unwrap(), 6);
        assert_eq!(r.b3().unwrap(), 7);
        let data = bytes(&then("10", &[0x34, 0x12]));
        assert_eq!(BitReader::new(&data).bll().unwrap(), 0x1234);
    }

    #[test]
    fn texts_in_both_kinds_with_their_limits() {
        let limits = ImportLimits::default();
        let data = bytes(&then("01 00000011", b"abc"));
        assert_eq!(BitReader::new(&data).text(&limits).unwrap(), b"abc");
        let data = bytes(&then("01 00000100", b"ab\0\0"));
        assert_eq!(BitReader::new(&data).text(&limits).unwrap(), b"ab");
        let data = bytes(&then("01 00000010", &[0x61, 0x00, 0xe9, 0x00]));
        assert_eq!(BitReader::new(&data).text_unicode(&limits).unwrap(), "aé");
        let small = ImportLimits {
            max_string_bytes: 2,
            ..limits
        };
        let data = bytes(&then("01 00000011", b"abc"));
        assert_eq!(
            BitReader::new(&data).text(&small),
            Err(DwgError::TooMany {
                what: "bytes in a string",
                limit: 2
            })
        );
        let data = bytes("01 00000011 01100001");
        assert!(matches!(
            BitReader::new(&data).text(&limits),
            Err(DwgError::Truncated { .. })
        ));
        let data = bytes("00 11111111 11111111");
        assert!(matches!(
            BitReader::new(&data).text(&limits),
            Err(DwgError::Corrupt { .. })
        ));
    }

    #[test]
    fn entity_colours_in_both_encodings() {
        let limits = ImportLimits::default();
        // R2000: a plain index.
        let data = bytes("01 00000011");
        assert_eq!(BitReader::new(&data).enc(Version::R2000).unwrap().index, 3);
        // R2004: flags 0x80 then an RGB bit long, 0x20 then a transparency.
        let mut stream = then("00 00000011 10100000 00", &[0x40, 0x30, 0x20, 0x00]);
        stream = then(&format!("{stream} 00"), &[0x7f, 0, 0, 3]);
        let data = bytes(&stream);
        let c = BitReader::new(&data).enc(Version::R2004).unwrap();
        assert_eq!(c.index, 3);
        assert_eq!(c.rgb, Some(0x20_3040));
        assert_eq!(c.transparency, Some(0x0300_007f));
        assert!(!c.has_book_handle);
        // CMC in R2004: an index of 0, then the long with its method byte:
        // a true colour, then an index, then by layer.
        let data = bytes(&then("10 00", &[0x01, 0x02, 0x03, 0xc2, 0x00]));
        let c = BitReader::new(&data).cmc(Version::R2004, &limits).unwrap();
        assert_eq!(c.index, 0);
        assert_eq!(c.rgb, Some(0x030201));
        let data = bytes(&then("10 00", &[0x07, 0x00, 0x00, 0xc3, 0x00]));
        let c = BitReader::new(&data).cmc(Version::R2004, &limits).unwrap();
        assert_eq!((c.index, c.rgb), (7, None));
        let data = bytes(&then("10 00", &[0x00, 0x00, 0x00, 0xc0, 0x00]));
        let c = BitReader::new(&data).cmc(Version::R2004, &limits).unwrap();
        assert_eq!((c.index, c.rgb), (256, None));
        let data = bytes("01 00000101");
        let c = BitReader::new(&data).cmc(Version::R2000, &limits).unwrap();
        assert_eq!(c.index, 5);
        assert_eq!(c.rgb, None);
    }

    #[test]
    fn reading_past_the_end_is_an_error_at_the_byte() {
        let data = [0xffu8; 2];
        let mut r = BitReader::new(&data);
        assert_eq!(r.rs().unwrap(), 0xffff);
        assert_eq!(
            r.bit(),
            Err(DwgError::Truncated {
                what: "a bit stream",
                at: 2
            })
        );
        assert!(r.window(8, 32).is_err());
        let mut w = r.window(4, 12).unwrap();
        assert_eq!(w.remaining_bits(), 8);
        assert_eq!(w.rc().unwrap(), 0xff);
        assert!(w.bit().is_err());
        w.align();
        assert_eq!(w.position(), 16);
    }
}
