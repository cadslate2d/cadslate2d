//! The R2004 container (ODA spec §4): an encrypted file header, a page map
//! and a section map in system pages, and data sections cut into
//! compressed pages with masked headers.

use crate::crc::{checksum_2004, crc32};
use crate::lz77::decompress_2004;
use crate::sections::{Section, SectionName};
use crate::{DwgError, ImportLimits};

const SYSTEM_PAGE_MAP: u32 = 0x4163_0e3b;
const SYSTEM_SECTION_MAP: u32 = 0x4163_003b;
const DATA_PAGE: u32 = 0x4163_043b;

/// The magic sequence the file header is XORed with (§4.1).
pub fn magic_sequence(n: usize) -> Vec<u8> {
    let mut seed: u32 = 1;
    (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            (seed >> 16) as u8
        })
        .collect()
}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn le64(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(d[at..at + 8].try_into().expect("eight bytes"))
}

fn slice<'a>(d: &'a [u8], at: usize, len: usize, what: &'static str) -> Result<&'a [u8], DwgError> {
    at.checked_add(len)
        .and_then(|end| d.get(at..end))
        .ok_or(DwgError::Truncated { what, at })
}

/// A page of the page map (§4.4): its number, where it is, how long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub number: i32,
    pub address: usize,
    pub size: usize,
}

/// A data section of the section map (§4.5).
#[derive(Clone, Debug)]
pub struct Described {
    pub name: String,
    pub size: usize,
    pub page_size: usize,
    pub compressed: bool,
    /// Page number, data size, start offset.
    pub pages: Vec<(u32, usize, usize)>,
}

/// Reads a system page (§4.3): its header, then LZ77 data checked against
/// the page checksum.
fn system_page(d: &[u8], address: usize, limits: &ImportLimits) -> Result<Vec<u8>, DwgError> {
    let header = slice(d, address, 0x14, "a system page")?;
    let kind = le32(header, 0);
    if kind != SYSTEM_PAGE_MAP && kind != SYSTEM_SECTION_MAP {
        return Err(DwgError::Corrupt {
            what: "a system page type",
            at: address,
        });
    }
    let decompressed = le32(header, 4) as usize;
    let compressed = le32(header, 8) as usize;
    let method = le32(header, 12);
    let stored = le32(header, 16);
    if method != 2 {
        return Err(DwgError::Corrupt {
            what: "a system page compression type",
            at: address + 12,
        });
    }
    if decompressed > limits.max_section_bytes {
        return Err(DwgError::TooMany {
            what: "bytes in a system page",
            limit: limits.max_section_bytes,
        });
    }
    let data = slice(d, address + 0x14, compressed, "a system page")?;
    // §4.3: the checksum of the header with its field zero seeds the
    // checksum of the data.
    let mut zeroed = header.to_vec();
    zeroed[16..20].fill(0);
    let seed = checksum_2004(0, &zeroed);
    if checksum_2004(seed, data) != stored {
        return Err(DwgError::Checksum {
            what: "a system page",
            at: address,
        });
    }
    let out = decompress_2004(data, decompressed)?;
    if out.len() != decompressed {
        return Err(DwgError::Corrupt {
            what: "a system page shorter than its size",
            at: address,
        });
    }
    Ok(out)
}

/// The decrypted file header (§4.1), checked.
fn file_header(d: &[u8]) -> Result<Vec<u8>, DwgError> {
    let raw = slice(d, 0x80, 0x6c, "the file header")?;
    let header: Vec<u8> = raw
        .iter()
        .zip(magic_sequence(0x6c))
        .map(|(a, b)| a ^ b)
        .collect();
    if &header[..11] != b"AcFssFcAJMB" {
        return Err(DwgError::Corrupt {
            what: "the file header id",
            at: 0x80,
        });
    }
    // §4.1: the CRC32 over the header with the CRC itself zero.
    let mut zeroed = header.clone();
    zeroed[0x68..0x6c].fill(0);
    if crc32(0, &zeroed) != le32(&header, 0x68) {
        return Err(DwgError::Checksum {
            what: "the file header",
            at: 0x80 + 0x68,
        });
    }
    Ok(header)
}

/// The page map and the section map of a file (§4.4, §4.5).
pub fn pages_and_sections(
    d: &[u8],
    limits: &ImportLimits,
) -> Result<(Vec<Page>, Vec<Described>), DwgError> {
    let header = file_header(d)?;
    let page_map_address = usize::try_from(le64(&header, 0x54))
        .ok()
        .and_then(|a| a.checked_add(0x100))
        .ok_or(DwgError::Corrupt {
            what: "the page map address",
            at: 0x80 + 0x54,
        })?;
    let section_map_id = le32(&header, 0x5c) as i32;
    let map = system_page(d, page_map_address, limits)?;
    let mut pages = Vec::new();
    let mut address = 0x100usize;
    let mut p = 0;
    while p + 8 <= map.len() {
        if pages.len() >= limits.max_pages {
            return Err(DwgError::TooMany {
                what: "pages",
                limit: limits.max_pages,
            });
        }
        let number = le32(&map, p) as i32;
        let size = le32(&map, p + 4) as usize;
        p += 8;
        if number < 0 {
            p += 16;
        }
        pages.push(Page {
            number,
            address,
            size,
        });
        address = address.checked_add(size).ok_or(DwgError::Corrupt {
            what: "a page size",
            at: page_map_address,
        })?;
    }
    let smap_page =
        pages
            .iter()
            .find(|pg| pg.number == section_map_id)
            .ok_or(DwgError::MissingSection {
                name: "section map",
            })?;
    let smap = system_page(d, smap_page.address, limits)?;
    let count = slice(&smap, 0, 4, "the section map").map(|b| le32(b, 0))? as usize;
    if count > limits.max_sections {
        return Err(DwgError::TooMany {
            what: "sections",
            limit: limits.max_sections,
        });
    }
    let mut sections = Vec::with_capacity(count);
    let mut p = 0x14;
    for _ in 0..count {
        let entry = slice(&smap, p, 0x60, "the section map")?;
        let size = usize::try_from(le64(entry, 0)).map_err(|_| DwgError::Corrupt {
            what: "a section size",
            at: p,
        })?;
        let page_count = le32(entry, 8) as usize;
        let page_size = le32(entry, 12) as usize;
        let compressed = le32(entry, 0x14) == 2;
        let name_bytes = &entry[0x20..0x60];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
        if page_count > limits.max_pages {
            return Err(DwgError::TooMany {
                what: "pages in a section",
                limit: limits.max_pages,
            });
        }
        if size > limits.max_section_bytes {
            return Err(DwgError::TooMany {
                what: "bytes in a section",
                limit: limits.max_section_bytes,
            });
        }
        p += 0x60;
        let mut section_pages = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            let page = slice(&smap, p, 16, "the section map")?;
            let start = usize::try_from(le64(page, 8)).map_err(|_| DwgError::Corrupt {
                what: "a page offset",
                at: p,
            })?;
            section_pages.push((le32(page, 0), le32(page, 4) as usize, start));
            p += 16;
        }
        sections.push(Described {
            name,
            size,
            page_size,
            compressed,
            pages: section_pages,
        });
    }
    Ok((pages, sections))
}

/// Assembles a data section from its pages (§4.6), each page's masked
/// header and checksums checked.
fn section(d: &[u8], pages: &[Page], s: &Described) -> Result<Vec<u8>, DwgError> {
    let mut out = vec![0u8; s.size];
    for &(number, data_size, start) in &s.pages {
        let page = pages
            .iter()
            .find(|p| p.number == number as i32)
            .ok_or(DwgError::Corrupt {
                what: "a section's page number",
                at: 0,
            })?;
        let a = page.address;
        let raw = slice(d, a, 32, "a data page header")?;
        let mask = 0x4164_536bu32 ^ a as u32;
        let mut header = [0u8; 32];
        for k in 0..8 {
            let v = le32(raw, k * 4) ^ mask;
            header[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        if le32(&header, 0) != DATA_PAGE {
            return Err(DwgError::Corrupt {
                what: "a data page type",
                at: a,
            });
        }
        let data = slice(d, a + 32, data_size, "a data page")?;
        let data_checksum = le32(&header, 0x1c);
        if checksum_2004(0, data) != data_checksum {
            return Err(DwgError::Checksum {
                what: "a data page",
                at: a,
            });
        }
        let mut zeroed = header;
        zeroed[0x18..0x1c].fill(0);
        if checksum_2004(data_checksum, &zeroed) != le32(&header, 0x18) {
            return Err(DwgError::Checksum {
                what: "a data page header",
                at: a,
            });
        }
        if start > s.size {
            return Err(DwgError::Corrupt {
                what: "a page's start offset",
                at: a,
            });
        }
        // §4.5: a page holds the section's page size, the last one padded;
        // only the logical prefix is the section's.
        let want = (s.size - start).min(s.page_size);
        let bytes = if s.compressed {
            decompress_2004(data, s.page_size.max(want))?
        } else {
            data.to_vec()
        };
        let bytes = bytes.get(..want).ok_or(DwgError::Corrupt {
            what: "a data page shorter than its section needs",
            at: a,
        })?;
        out[start..start + want].copy_from_slice(bytes);
    }
    Ok(out)
}

/// Every data section of the file, decoded.
pub fn sections(bytes: &[u8], limits: &ImportLimits) -> Result<Vec<Section>, DwgError> {
    let (pages, described) = pages_and_sections(bytes, limits)?;
    described
        .iter()
        .map(|s| {
            Ok(Section {
                name: SectionName::parse(&s.name),
                data: section(bytes, &pages, s)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_magic_sequence_starts_as_the_specification_prints_it() {
        assert_eq!(
            magic_sequence(8),
            [0x29, 0x23, 0xBE, 0x84, 0xE1, 0x6C, 0xD6, 0xAE]
        );
    }

    #[test]
    fn a_file_too_short_for_its_header_is_truncated_not_a_panic() {
        let limits = ImportLimits::default();
        assert!(matches!(
            sections(b"AC1018", &limits),
            Err(DwgError::Truncated { .. })
        ));
        let mut bytes = vec![0u8; 0x100];
        bytes[..6].copy_from_slice(b"AC1018");
        assert!(matches!(
            sections(&bytes, &limits),
            Err(DwgError::Corrupt { .. })
        ));
    }
}
