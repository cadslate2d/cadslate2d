//! The bit codes of ODA spec §2, written: the mirror of `bits.rs`, most
//! significant bit first, producing the compressed forms every section and
//! object is read from. What this writes, `BitReader` reads back exactly;
//! the property tests at the end hold it to that.

use cad_core::Point2;

/// A writer of bit codes into bytes, positioned in bits.
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    /// Bits used in the last byte, 0 when the last byte is full (or there
    /// is none).
    bit: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The position in bits from the start.
    pub fn position(&self) -> usize {
        match self.bit {
            0 => self.bytes.len() * 8,
            bit => (self.bytes.len() - 1) * 8 + usize::from(bit),
        }
    }

    /// Pads the last byte with zero bits.
    pub fn align(&mut self) {
        self.bit = 0;
    }

    /// The bytes, the last padded with zero bits.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Appends another writer's bits at the current bit position.
    pub fn append(&mut self, other: &BitWriter) {
        let bits = other.position();
        for i in 0..bits {
            let byte = other.bytes[i / 8];
            self.bit((byte >> (7 - (i % 8))) & 1);
        }
    }

    /// B: one bit.
    pub fn bit(&mut self, value: u8) {
        if self.bit == 0 {
            self.bytes.push(0);
        }
        if value & 1 != 0 {
            let last = self.bytes.len() - 1;
            self.bytes[last] |= 0x80 >> self.bit;
        }
        self.bit = (self.bit + 1) % 8;
    }

    /// `n` bits of `value`, most significant first.
    pub fn bits(&mut self, n: u32, value: u64) {
        for i in (0..n).rev() {
            self.bit(((value >> i) & 1) as u8);
        }
    }

    /// BB: two bits.
    pub fn bb(&mut self, value: u8) {
        self.bits(2, u64::from(value & 3));
    }

    /// RC: a raw byte.
    pub fn rc(&mut self, value: u8) {
        self.bits(8, u64::from(value));
    }

    /// RS: a raw short, little-endian.
    pub fn rs(&mut self, value: u16) {
        for b in value.to_le_bytes() {
            self.rc(b);
        }
    }

    /// RL: a raw long, little-endian.
    pub fn rl(&mut self, value: u32) {
        for b in value.to_le_bytes() {
            self.rc(b);
        }
    }

    /// RLL: a raw long long, little-endian.
    pub fn rll(&mut self, value: u64) {
        for b in value.to_le_bytes() {
            self.rc(b);
        }
    }

    /// RD: a raw double, little-endian.
    pub fn rd(&mut self, value: f64) {
        for b in value.to_le_bytes() {
            self.rc(b);
        }
    }

    pub fn rd2(&mut self, point: Point2) {
        self.rd(point.x);
        self.rd(point.y);
    }

    /// BS: a bit short (§2.2).
    pub fn bs(&mut self, value: i16) {
        match value {
            0 => self.bb(2),
            256 => self.bb(3),
            1..=255 => {
                self.bb(1);
                self.rc(value as u8);
            }
            _ => {
                self.bb(0);
                self.rs(value as u16);
            }
        }
    }

    /// BL: a bit long (§2.3).
    pub fn bl(&mut self, value: i32) {
        match value {
            0 => self.bb(2),
            1..=255 => {
                self.bb(1);
                self.rc(value as u8);
            }
            _ => {
                self.bb(0);
                self.rl(value as u32);
            }
        }
    }

    /// BD: a bit double (§2.5).
    pub fn bd(&mut self, value: f64) {
        if value.to_bits() == 1.0f64.to_bits() {
            self.bb(1);
        } else if value.to_bits() == 0.0f64.to_bits() {
            self.bb(2);
        } else {
            self.bb(0);
            self.rd(value);
        }
    }

    pub fn bd2(&mut self, point: Point2) {
        self.bd(point.x);
        self.bd(point.y);
    }

    pub fn bd3(&mut self, point: Point2, z: f64) {
        self.bd2(point);
        self.bd(z);
    }

    /// DD: a bit double with a default (§2.9), in the shortest form that
    /// reads back to `value` exactly.
    pub fn dd(&mut self, default: f64, value: f64) {
        let d = default.to_le_bytes();
        let v = value.to_le_bytes();
        if d == v {
            self.bb(0);
        } else if d[4..] == v[4..] {
            self.bb(1);
            for b in &v[..4] {
                self.rc(*b);
            }
        } else if d[6..] == v[6..] {
            self.bb(2);
            self.rc(v[4]);
            self.rc(v[5]);
            for b in &v[..4] {
                self.rc(*b);
            }
        } else {
            self.bb(3);
            self.rd(value);
        }
    }

    /// BT: a thickness (§2.10).
    pub fn bt(&mut self, value: f64) {
        if value.to_bits() == 0 {
            self.bit(1);
        } else {
            self.bit(0);
            self.bd(value);
        }
    }

    /// BE: an extrusion (§2.8).
    pub fn be(&mut self, extrusion: [f64; 3]) {
        if extrusion.map(f64::to_bits) == [0.0, 0.0, 1.0].map(f64::to_bits) {
            self.bit(1);
        } else {
            self.bit(0);
            for v in extrusion {
                self.bd(v);
            }
        }
    }

    /// MC: a modular char (§2.6), signed by the 0x40 bit of the last byte.
    pub fn mc(&mut self, value: i64) {
        let negative = value < 0;
        let mut v = value.unsigned_abs();
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 && byte & 0x40 == 0 {
                if negative {
                    byte |= 0x40;
                }
                self.rc(byte);
                return;
            }
            self.rc(byte | 0x80);
        }
    }

    /// A modular char without the sign bit: the object map's handle
    /// offsets (§2.6).
    pub fn umc(&mut self, value: u64) {
        let mut v = value;
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.rc(byte);
                return;
            }
            self.rc(byte | 0x80);
        }
    }

    /// MS: a modular short (§2.7).
    pub fn ms(&mut self, value: u32) {
        let mut v = value;
        loop {
            let word = (v & 0x7fff) as u16;
            v >>= 15;
            if v == 0 {
                self.rs(word);
                return;
            }
            self.rs(word | 0x8000);
        }
    }

    /// H: a handle reference (§2.13): the code, the byte count, the bytes
    /// most significant first, as few as the value needs.
    pub fn handle(&mut self, code: u8, value: u64) {
        let bytes = value.to_be_bytes();
        let n = 8 - (value.leading_zeros() / 8) as usize;
        self.bits(4, u64::from(code & 0xf));
        self.bits(4, n as u64);
        for b in &bytes[8 - n..] {
            self.rc(*b);
        }
    }

    /// T: a text of the R2000 and R2004 kind — a bit short length and the
    /// bytes. The drawing's code page is not applied: the bytes are what
    /// the DXF of this release carries, UTF-8.
    pub fn text(&mut self, value: &str) {
        let bytes = value.as_bytes();
        let n = bytes.len().min(i16::MAX as usize);
        self.bs(n as i16);
        for b in &bytes[..n] {
            self.rc(*b);
        }
    }

    /// TU: a Unicode text of the R2007 kind — a bit short count of UTF-16
    /// units, then the units.
    pub fn text_unicode(&mut self, value: &str) {
        let units: Vec<u16> = value.encode_utf16().collect();
        let n = units.len().min(i16::MAX as usize);
        self.bs(n as i16);
        for unit in &units[..n] {
            self.rs(*unit);
        }
    }

    /// CMC of R2000 (§2.11): the colour index alone.
    pub fn cmc(&mut self, index: i16) {
        self.bs(index);
    }

    /// ENC of R2000 (§2.11): the colour index alone.
    pub fn enc(&mut self, index: i16) {
        self.bs(index);
    }

    /// Patches a raw long written earlier at a byte position.
    pub fn patch_rl(&mut self, at_byte: usize, value: u32) {
        self.bytes[at_byte..at_byte + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Raw bytes, at a byte boundary.
    pub fn bytes(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.bit, 0, "bytes go at a byte boundary");
        self.bytes.extend_from_slice(bytes);
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::bits::BitReader;
    use crate::{ImportLimits, Version};

    fn reader(w: &BitWriter) -> BitReader<'_> {
        BitReader::new(&w.bytes)
    }

    #[test]
    fn the_fixed_forms_read_back() {
        let mut w = BitWriter::new();
        w.bit(1);
        w.bb(2);
        w.rc(0xAB);
        w.rs(0x1234);
        w.rl(0xDEAD_BEEF);
        w.rll(0x0102_0304_0506_0708);
        w.rd(-2.5);
        w.align();
        let mut r = reader(&w);
        assert_eq!(r.bit().unwrap(), 1);
        assert_eq!(r.bb().unwrap(), 2);
        assert_eq!(r.rc().unwrap(), 0xAB);
        assert_eq!(r.rs().unwrap(), 0x1234);
        assert_eq!(r.rl().unwrap(), 0xDEAD_BEEF);
        assert_eq!(r.rll().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(r.rd().unwrap(), -2.5);
    }

    #[test]
    fn append_joins_at_the_bit() {
        let mut a = BitWriter::new();
        a.bits(3, 0b101);
        let mut b = BitWriter::new();
        b.bits(7, 0b1100110);
        a.append(&b);
        assert_eq!(a.position(), 10);
        let mut r = reader(&a);
        assert_eq!(r.bits(3).unwrap(), 0b101);
        assert_eq!(r.bits(7).unwrap(), 0b1100110);
    }

    #[test]
    fn handles_take_as_few_bytes_as_the_value_needs() {
        let mut w = BitWriter::new();
        w.handle(5, 0);
        w.handle(4, 0x1F);
        w.handle(3, 0x0012_3456);
        assert_eq!(w.position(), 8 + 16 + 32);
        let mut r = reader(&w);
        let h = r.handle().unwrap();
        assert_eq!((h.code, h.value), (5, 0));
        let h = r.handle().unwrap();
        assert_eq!((h.code, h.value), (4, 0x1F));
        let h = r.handle().unwrap();
        assert_eq!((h.code, h.value), (3, 0x0012_3456));
    }

    proptest! {
        #[test]
        fn bit_shorts_and_longs_read_back(s in any::<i16>(), l in any::<i32>()) {
            let mut w = BitWriter::new();
            w.bs(s);
            w.bl(l);
            let mut r = reader(&w);
            prop_assert_eq!(r.bs().unwrap(), s);
            prop_assert_eq!(r.bl().unwrap(), l);
        }

        #[test]
        fn doubles_read_back(a in any::<f64>(), b in any::<f64>(), c in any::<f64>()) {
            prop_assume!(a.is_finite() && b.is_finite() && c.is_finite());
            let mut w = BitWriter::new();
            w.bd(a);
            w.dd(a, b);
            w.dd(b, b);
            w.dd(a, c);
            w.bt(c);
            w.be([a, b, c]);
            w.be([0.0, 0.0, 1.0]);
            let mut r = reader(&w);
            prop_assert_eq!(r.bd().unwrap().to_bits(), a.to_bits());
            prop_assert_eq!(r.dd(a).unwrap().to_bits(), b.to_bits());
            prop_assert_eq!(r.dd(b).unwrap().to_bits(), b.to_bits());
            prop_assert_eq!(r.dd(a).unwrap().to_bits(), c.to_bits());
            prop_assert_eq!(r.bt().unwrap().to_bits(), c.to_bits());
            prop_assert_eq!(r.be().unwrap(), [a, b, c]);
            prop_assert_eq!(r.be().unwrap(), [0.0, 0.0, 1.0]);
        }

        #[test]
        fn modular_forms_read_back(m in -(1i64 << 40)..(1i64 << 40), u in 0u64..(1 << 56), s in 0u32..(1 << 29)) {
            let mut w = BitWriter::new();
            w.mc(m);
            w.umc(u);
            w.ms(s);
            let mut r = reader(&w);
            prop_assert_eq!(r.mc().unwrap(), m);
            prop_assert_eq!(r.umc().unwrap(), u);
            prop_assert_eq!(r.ms().unwrap(), s);
        }

        #[test]
        fn handles_and_texts_read_back(code in 0u8..16, value in any::<u64>(), text in "[a-zA-Z0-9 _*$.-]{0,40}") {
            let mut w = BitWriter::new();
            w.handle(code, value);
            w.text(&text);
            w.cmc(7);
            let mut r = reader(&w);
            let h = r.handle().unwrap();
            prop_assert_eq!((h.code, h.value), (code, value));
            let limits = ImportLimits::default();
            prop_assert_eq!(r.text(&limits).unwrap(), text.as_bytes());
            prop_assert_eq!(r.cmc(Version::R2000, &limits).unwrap().index, 7);
        }
    }
}
