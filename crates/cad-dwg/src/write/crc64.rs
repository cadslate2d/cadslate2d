//! The 64-bit CRCs of an R2007 file (ODA spec §5.12) and the CRC random
//! encoding that seeds them and pads pages (§5.11). Neither reader this
//! crate is tested against checks them; the file carries them as the
//! specification lays them out.

/// The normal CRC's polynomial (ECMA-182), shifted left.
const NORMAL_POLY: u64 = 0x42F0_E1EB_A9EA_3693;
/// The mirrored CRC's polynomial, reflected, shifted right.
const MIRRORED_POLY: u64 = 0x95AC_9329_AC4B_C9B5;

fn normal_table() -> [u64; 256] {
    let mut table = [0u64; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut crc = (i as u64) << 56;
        for _ in 0..8 {
            crc = if crc & (1 << 63) != 0 {
                (crc << 1) ^ NORMAL_POLY
            } else {
                crc << 1
            };
        }
        *entry = crc;
    }
    table
}

fn mirrored_table() -> [u64; 256] {
    let mut table = [0u64; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut crc = i as u64;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ MIRRORED_POLY
            } else {
                crc >> 1
            };
        }
        *entry = crc;
    }
    table
}

/// The byte order the CRCs take their input in (§5.12): each eight bytes
/// as four pairs from the last, a remainder by its own table.
fn reordered(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let blocks = data.len() / 8;
    for b in 0..blocks {
        let at = b * 8;
        for offset in [6, 7, 4, 5, 2, 3, 0, 1] {
            out.push(data[at + offset]);
        }
    }
    let rest = &data[blocks * 8..];
    let order: &[usize] = match rest.len() {
        0 => &[],
        1 => &[0],
        2 => &[0, 1],
        3 => &[0, 1, 2],
        4 => &[2, 3, 0, 1],
        5 => &[2, 3, 0, 1, 4],
        6 => &[2, 3, 0, 1, 4, 5],
        _ => &[2, 3, 0, 1, 4, 5, 6],
    };
    for &o in order {
        out.push(rest[o]);
    }
    out
}

fn normal_raw(seed: u64, data: &[u8]) -> u64 {
    let table = normal_table();
    data.iter().fold(seed, |crc, &byte| {
        table[usize::from(byte ^ (crc >> 56) as u8)] ^ (crc << 8)
    })
}

fn mirrored_raw(seed: u64, data: &[u8]) -> u64 {
    let table = mirrored_table();
    data.iter().fold(seed, |crc, &byte| {
        table[usize::from((crc as u8) ^ byte)] ^ (crc >> 8)
    })
}

/// `UpdateSeed1` (§5.12).
pub fn update_seed1(seed: u64, length: u32) -> u64 {
    let len = u64::from(length);
    let mut s = seed
        .wrapping_add(len)
        .wrapping_mul(0x343fd)
        .wrapping_add(0x269ec3);
    s |= s.wrapping_mul(0x343fd << 32).wrapping_add(0x269ec3 << 32);
    !s
}

/// `UpdateSeed2` (§5.12).
pub fn update_seed2(seed: u64, length: u32) -> u64 {
    let len = u64::from(length);
    let s = seed
        .wrapping_add(len)
        .wrapping_mul(0x343fd)
        .wrapping_add(0x269ec3);
    let s = s
        .wrapping_mul((1 << 32) + 0x343fd)
        .wrapping_add(len.wrapping_add(0x269ec3));
    !s
}

/// The normal CRC of `data`, its seed updated by `UpdateSeed2`, inverted
/// at the end: the file header's.
pub fn normal(seed: u64, data: &[u8]) -> u64 {
    !normal_raw(update_seed2(seed, data.len() as u32), &reordered(data))
}

/// The normal CRC with `UpdateSeed1`: the checking sequence's (§5.2.1.4).
pub fn normal_seed1(seed: u64, data: &[u8]) -> u64 {
    !normal_raw(update_seed1(seed, data.len() as u32), &reordered(data))
}

/// The mirrored CRC of `data`, its seed updated by `UpdateSeed1`: a
/// page's (§5.3, §5.4).
pub fn mirrored(seed: u64, data: &[u8]) -> u64 {
    mirrored_raw(update_seed1(seed, data.len() as u32), &reordered(data))
}

/// The check data's normal CRC (§5.2.1.1.5): seeded by the inverted
/// second random value, inverted at the end.
pub fn check_normal(random2: u64, data: &[u8]) -> u64 {
    !normal_raw(!random2, &reordered(data))
}

/// The check data's mirrored CRC: seeded by the inverted first random
/// value.
pub fn check_mirrored(random1: u64, data: &[u8]) -> u64 {
    mirrored_raw(!random1, &reordered(data))
}

/// The check data's `Encode` (§5.2.1.1.5): a rotation by the control's
/// low five bits.
pub fn rotate(value: u64, control: u64) -> u64 {
    let shift = (control & 0x1f) as u32;
    if shift == 0 {
        value
    } else {
        value.rotate_left(shift)
    }
}

/// The CRC random encoder (§5.11): a table of 0x270 longs from a seed, an
/// index through it, and a padding table drawn from it first.
pub struct RandomEncoder {
    table: Vec<u32>,
    index: usize,
    padding: Vec<u32>,
    padding_used: usize,
}

impl RandomEncoder {
    pub fn new(seed: u64) -> Self {
        let mut table = vec![0u32; 0x270];
        table[0] = (seed as u32).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        table[1] = ((seed >> 32) as u32)
            .wrapping_mul(0x343fd)
            .wrapping_add(0x269ec3);
        let mut value = table[1];
        for (i, entry) in table.iter_mut().enumerate().skip(2) {
            value = ((value >> 30) ^ value)
                .wrapping_mul(0x6c07_8965)
                .wrapping_add(i as u32);
            *entry = value;
        }
        let mut encoder = Self {
            table,
            index: 0,
            padding: Vec::with_capacity(0x80),
            padding_used: 0,
        };
        for _ in 0..0x80 {
            encoder.wrap();
            let value = encoder.table[encoder.index];
            encoder.padding.push(value);
            encoder.index += 1;
        }
        encoder
    }

    fn wrap(&mut self) {
        if self.index >= 0x270 {
            self.index = 0;
        }
    }

    /// The next random long.
    pub fn next_u64(&mut self) -> u64 {
        self.index += 2;
        self.wrap();
        let lo = u64::from(self.table[self.index]);
        let hi = u64::from(self.table[(self.index + 1) % 0x270]);
        lo | (hi << 32)
    }

    /// Encodes the ten low bits of `value` into a random long (§5.11).
    pub fn encode(&mut self, value: u32) -> u64 {
        let random = self.next_u64();
        let mut lo = (random as u32) & 0xdf7d_f7df;
        let mut hi = ((random >> 32) as u32) & 0xf7df_7df7;
        for (bit, (mask_lo, mask_hi)) in [
            (0x200, (0x20, 0)),
            (0x100, (0x800, 0)),
            (0x80, (0x20000, 0)),
            (0x40, (0x80_0000, 0)),
            (0x20, (0x2000_0000, 0)),
            (0x10, (0, 0x08)),
            (0x8, (0, 0x200)),
            (0x4, (0, 0x8000)),
            (0x2, (0, 0x20_0000)),
            (0x1, (0, 0x800_0000)),
        ] {
            if value & bit != 0 {
                lo |= mask_lo;
                hi |= mask_hi;
            }
        }
        u64::from(lo) | (u64::from(hi) << 32)
    }

    /// Fills `buffer` with padding: the padding table first, then the
    /// encoding table on.
    pub fn fill(&mut self, buffer: &mut [u8]) {
        let mut at = 0;
        while at < buffer.len() {
            let value = if self.padding_used < self.padding.len() {
                let v = self.padding[self.padding_used];
                self.padding_used += 1;
                v
            } else {
                self.index += 1;
                self.wrap();
                self.table[self.index]
            };
            let bytes = value.to_le_bytes();
            let n = (buffer.len() - at).min(4);
            buffer[at..at + n].copy_from_slice(&bytes[..n]);
            at += n;
        }
    }
}

/// The data page checksum of R2007 (§5.4.1): the R2004 sums over the bytes
/// in the CRC byte order, from a seed stirred with the length.
pub fn page_checksum(seed: u32, data: &[u8]) -> u32 {
    let seed = u64::from(seed)
        .wrapping_add(data.len() as u64)
        .wrapping_mul(0x343fd)
        .wrapping_add(0x269ec3);
    let mut sum1 = (seed & 0xffff) as u32;
    let mut sum2 = ((seed >> 16) & 0xffff) as u32;
    for chunk in data.chunks(0x15b0) {
        for byte in reordered(chunk) {
            sum1 += u32::from(byte);
            sum2 += sum1;
        }
        sum1 %= 0xfff1;
        sum2 %= 0xfff1;
    }
    (sum2 << 16) | (sum1 & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_the_specifications() {
        let normal = normal_table();
        assert_eq!(normal[1], 0x42f0_e1eb_a9ea_3693);
        assert_eq!(normal[2], 0x85e1_c3d7_53d4_6d26);
        assert_eq!(normal[255], 0x9afc_e626_ce85_b507);
        let mirrored = mirrored_table();
        assert_eq!(mirrored[1], 0x7ad8_70c8_3035_8979);
        assert_eq!(mirrored[0x80], 0x95ac_9329_ac4b_c9b5);
        assert_eq!(mirrored[0x7f], 0xbc1b_436e_43a7_4e9d);
    }

    #[test]
    fn the_byte_order_is_the_specifications_table() {
        assert_eq!(
            reordered(&[1, 2, 3, 4, 5, 6, 7, 8]),
            [7, 8, 5, 6, 3, 4, 1, 2]
        );
        assert_eq!(reordered(&[1, 2, 3, 4, 5, 6, 7]), [3, 4, 1, 2, 5, 6, 7]);
        assert_eq!(reordered(&[1, 2, 3]), [1, 2, 3]);
        assert_eq!(reordered(&[1, 2, 3, 4]), [3, 4, 1, 2]);
    }

    #[test]
    fn the_random_encoder_starts_as_the_specification_computes_it() {
        let mut encoder = RandomEncoder::new(0);
        // Table entries from the seed: 0x269ec3, 0x269ec3, then the recurrence.
        const FIRST: u32 = 0x0026_9ec3;
        assert_eq!(encoder.table[0], FIRST);
        assert_eq!(encoder.table[1], FIRST);
        let value = ((FIRST >> 30) ^ FIRST)
            .wrapping_mul(0x6c07_8965)
            .wrapping_add(2);
        assert_eq!(encoder.table[2], value);
        let first = encoder.next_u64();
        assert_ne!(first, 0);
        let encoded = encoder.encode(0x3ff);
        assert_eq!(encoded & 0x20, 0x20);
        assert_eq!((encoded >> 32) & 0x800_0000, 0x800_0000);
        let mut buffer = [0u8; 10];
        encoder.fill(&mut buffer);
        assert_eq!(&buffer[..4], &encoder.padding[0].to_le_bytes());
    }

    #[test]
    fn the_seeds_and_crcs_are_stable() {
        let s = 0x0026_9ec3_u64;
        assert_eq!(
            update_seed1(0, 0),
            !(s | s.wrapping_mul(0x343fd << 32).wrapping_add(0x269ec3 << 32))
        );
        assert_eq!(
            update_seed2(0, 0),
            !(s.wrapping_mul((1 << 32) + 0x343fd).wrapping_add(0x269ec3))
        );
        let data: Vec<u8> = (0..100).collect();
        assert_eq!(normal(0, &data), normal(0, &data));
        assert_ne!(normal(0, &data), mirrored(0, &data));
        assert_ne!(page_checksum(0, &data), 0);
    }
}
