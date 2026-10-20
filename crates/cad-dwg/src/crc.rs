//! The checksums of the containers (ODA spec §2.14, §4.2).

/// The 16-bit CRC of §2.14.1 (the table there is CRC-16/ARC's), run from
/// `seed` over `data`.
pub fn crc16(seed: u16, data: &[u8]) -> u16 {
    let mut crc = seed;
    for &byte in data {
        let index = (crc ^ u16::from(byte)) & 0xff;
        crc = (crc >> 8) ^ CRC16_TABLE[index as usize];
    }
    crc
}

const CRC16_TABLE: [u16; 256] = {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u16;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// The 32-bit CRC of §2.14.2 (the table there is the IEEE one), run from
/// `seed` over `data`.
pub fn crc32(seed: u32, data: &[u8]) -> u32 {
    let mut crc = !seed;
    for &byte in data {
        let index = (crc ^ u32::from(byte)) & 0xff;
        crc = (crc >> 8) ^ CRC32_TABLE[index as usize];
    }
    !crc
}

const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// The R2004 section page checksum (§4.2): Adler-like sums over chunks of
/// 0x15b0 bytes, from `seed`.
pub fn checksum_2004(seed: u32, data: &[u8]) -> u32 {
    let mut sum1 = seed & 0xffff;
    let mut sum2 = seed >> 16;
    for chunk in data.chunks(0x15b0) {
        for &byte in chunk {
            sum1 += u32::from(byte);
            sum2 += sum1;
        }
        sum1 %= 0xFFF1;
        sum2 %= 0xFFF1;
    }
    (sum2 << 16) | (sum1 & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_the_specifications() {
        // §2.14.1's second and third entries, §2.14.2's second.
        assert_eq!(CRC16_TABLE[1], 0xC0C1);
        assert_eq!(CRC16_TABLE[2], 0xC181);
        assert_eq!(CRC16_TABLE[255], 0x4040);
        assert_eq!(CRC32_TABLE[1], 0x7707_3096);
        assert_eq!(CRC32_TABLE[255], 0x2D02_EF8D);
        // CRC-16/ARC and CRC-32 of "123456789", the check values.
        assert_eq!(crc16(0, b"123456789"), 0xBB3D);
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_page_checksums_are_stable() {
        assert_eq!(checksum_2004(0, &[]), 0);
        assert_eq!(checksum_2004(0, &[1]), (1 << 16) | 1);
        assert_eq!(checksum_2004(0, &[1, 2]), (4 << 16) | 3);
    }
}
