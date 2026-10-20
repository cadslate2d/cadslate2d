//! The R2000 container (ODA spec §3): a file header with section locators,
//! each section at an absolute address.

use crate::crc::crc16;
use crate::sections::{Section, SectionName};
use crate::{DwgError, ImportLimits};

/// The sentinel after the file header's CRC (§3.2.6).
const HEADER_SENTINEL: [u8; 16] = [
    0x95, 0xA0, 0x4E, 0x28, 0x99, 0x82, 0x1A, 0xE5, 0x5E, 0x41, 0xE0, 0x5F, 0x9D, 0x3A, 0x4D, 0x00,
];

/// A section locator record (§3.2.6): where a section is and how long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locator {
    pub number: u8,
    pub address: usize,
    pub size: usize,
}

fn le32(d: &[u8], at: usize, what: &'static str) -> Result<u32, DwgError> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(DwgError::Truncated { what, at })
}

/// Reads the section locators, checking the header's CRC and sentinel.
pub fn locators(bytes: &[u8], limits: &ImportLimits) -> Result<Vec<Locator>, DwgError> {
    let count = le32(bytes, 0x15, "the file header")? as usize;
    if count > limits.max_sections {
        return Err(DwgError::TooMany {
            what: "section locators",
            limit: limits.max_sections,
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = 0x19 + i * 9;
        let number = *bytes.get(at).ok_or(DwgError::Truncated {
            what: "the section locators",
            at,
        })?;
        let address = le32(bytes, at + 1, "the section locators")? as usize;
        let size = le32(bytes, at + 5, "the section locators")? as usize;
        if address
            .checked_add(size)
            .is_none_or(|end| end > bytes.len())
        {
            return Err(DwgError::Corrupt {
                what: "a section locator",
                at,
            });
        }
        out.push(Locator {
            number,
            address,
            size,
        });
    }
    let end = 0x19 + count * 9;
    let stored = bytes
        .get(end..end + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(DwgError::Truncated {
            what: "the file header",
            at: end,
        })?;
    // §3.2.6: the CRC from the start of the file, XORed by the number of
    // locators.
    let magic = match count {
        3 => 0xA598,
        4 => 0x8101,
        5 => 0x3CC4,
        6 => 0x8461,
        _ => 0,
    };
    if crc16(0, &bytes[..end]) ^ magic != stored {
        return Err(DwgError::Checksum {
            what: "the file header",
            at: end,
        });
    }
    if bytes.get(end + 2..end + 18) != Some(&HEADER_SENTINEL[..]) {
        return Err(DwgError::Corrupt {
            what: "the file header sentinel",
            at: end + 2,
        });
    }
    Ok(out)
}

/// The sections an R2000 file has, by the locator numbers of §3.2.6.
pub fn sections(bytes: &[u8], limits: &ImportLimits) -> Result<Vec<Section>, DwgError> {
    locators(bytes, limits)?
        .into_iter()
        .map(|locator| {
            let name = match locator.number {
                0 => SectionName::Header,
                1 => SectionName::Classes,
                2 => SectionName::Handles,
                3 => SectionName::Unknown,
                4 => SectionName::Template,
                _ => SectionName::Other,
            };
            Ok(Section {
                name,
                data: bytes[locator.address..locator.address + locator.size].to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header with three empty locators and the right CRC and sentinel.
    pub fn header(count: u32) -> Vec<u8> {
        let mut bytes = b"AC1015".to_vec();
        bytes.extend([0u8; 0x15 - 6]);
        bytes.extend(count.to_le_bytes());
        for i in 0..count {
            bytes.push(i as u8);
            bytes.extend(0u32.to_le_bytes());
            bytes.extend(0u32.to_le_bytes());
        }
        let magic = match count {
            3 => 0xA598,
            4 => 0x8101,
            5 => 0x3CC4,
            6 => 0x8461,
            _ => 0,
        };
        let crc = crc16(0, &bytes) ^ magic;
        bytes.extend(crc.to_le_bytes());
        bytes.extend(HEADER_SENTINEL);
        bytes
    }

    #[test]
    fn locators_are_read_and_the_header_crc_and_sentinel_checked() {
        let limits = ImportLimits::default();
        let bytes = header(3);
        let found = locators(&bytes, &limits).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[2].number, 2);
        let mut bad = bytes.clone();
        bad[0x19 + 27] ^= 1;
        assert!(matches!(
            locators(&bad, &limits),
            Err(DwgError::Checksum { .. })
        ));
        let mut bad = bytes.clone();
        bad[0x19 + 29] ^= 1;
        assert!(matches!(
            locators(&bad, &limits),
            Err(DwgError::Corrupt { .. })
        ));
        assert!(matches!(
            locators(&bytes[..0x19 + 10], &limits),
            Err(DwgError::Truncated { .. })
        ));
        let small = ImportLimits {
            max_sections: 2,
            ..limits
        };
        assert_eq!(
            locators(&bytes, &small),
            Err(DwgError::TooMany {
                what: "section locators",
                limit: 2
            })
        );
        // A locator past the end of the file.
        let mut bad = header(3);
        bad[0x19 + 5..0x19 + 9].copy_from_slice(&1000u32.to_le_bytes());
        assert!(matches!(
            locators(&bad, &limits),
            Err(DwgError::Corrupt { .. })
        ));
    }
}
