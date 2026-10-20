//! The R2007 container (ODA spec §5): a file header and system pages
//! encoded with Reed–Solomon (255, 239), data pages with (255, 251), the
//! R21 compressor, and sections of pages found through a page map and a
//! section map.
//!
//! Reed–Solomon *correction* is not done: the parity bytes are dropped,
//! and a damaged page shows as a failed checksum or corrupt data.

use crate::lz77::decompress_2007;
use crate::sections::{Section, SectionName};
use crate::{DwgError, ImportLimits};

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

/// Undoes the interleaving of `blocks` codewords of 255 bytes and keeps
/// the `k` data bytes of each (§5.13). The caller has checked that the
/// source holds the blocks, so the output is at most its size.
fn rs_deinterleave(src: &[u8], blocks: usize, k: usize) -> Vec<u8> {
    debug_assert!(blocks * k <= src.len() + 255);
    let mut out = Vec::with_capacity((blocks * k).min(src.len() + 255));
    for b in 0..blocks {
        for j in 0..k {
            out.push(src.get(b + j * blocks).copied().unwrap_or(0));
        }
    }
    out
}

/// A system page (§5.3): `repeat` copies of the data, Reed–Solomon coded
/// in blocks of 239, compressed when shorter.
fn system_page(
    d: &[u8],
    at: usize,
    size_comp: usize,
    size_uncomp: usize,
    repeat: usize,
    limits: &ImportLimits,
) -> Result<Vec<u8>, DwgError> {
    if size_uncomp > limits.max_section_bytes || repeat > 64 {
        return Err(DwgError::TooMany {
            what: "bytes in a system page",
            limit: limits.max_section_bytes,
        });
    }
    // §5.3: the data is compressed only when that is shorter.
    if size_comp > size_uncomp {
        return Err(DwgError::Corrupt {
            what: "a system page's sizes",
            at,
        });
    }
    let pesize = size_comp.div_ceil(8) * 8 * repeat.max(1);
    let blocks = pesize.div_ceil(239);
    let page_size = (blocks * 255).div_ceil(8) * 8;
    let raw = slice(d, at, page_size, "a system page")?;
    let pedata = rs_deinterleave(raw, blocks, 239);
    if size_comp < size_uncomp {
        decompress_2007(&pedata[..size_comp.min(pedata.len())], size_uncomp)
    } else {
        slice(&pedata, 0, size_uncomp, "a system page").map(<[u8]>::to_vec)
    }
}

#[derive(Clone, Copy, Debug)]
struct Page {
    id: i64,
    address: usize,
    size: usize,
}

#[derive(Clone, Debug)]
struct Described {
    name: String,
    size: usize,
    pages: Vec<(usize, i64, usize, usize)>, // offset, id, uncompressed, compressed
}

fn pages_and_sections(
    d: &[u8],
    limits: &ImportLimits,
) -> Result<(Vec<Page>, Vec<Described>), DwgError> {
    // §5.2: 0x3d8 bytes at 0x80, three interleaved blocks of 239.
    let raw = slice(d, 0x80, 0x3d8, "the file header")?;
    let pedata = rs_deinterleave(raw, 3, 239);
    let compr_len = le32(&pedata, 24) as i32;
    let header = if compr_len > 0 {
        let n = (compr_len as usize).min(pedata.len() - 32);
        decompress_2007(&pedata[32..32 + n], 0x110)?
    } else {
        pedata[32..32 + 0x110].to_vec()
    };
    if le64(&header, 0) != 0x70 {
        return Err(DwgError::Corrupt {
            what: "the file header size",
            at: 0x80,
        });
    }
    let at = |offset: usize| -> Result<usize, DwgError> {
        usize::try_from(le64(&header, offset)).map_err(|_| DwgError::Corrupt {
            what: "a file header field",
            at: 0x80 + offset,
        })
    };
    let pages_map_correction = at(0x18)?;
    let pages_map_offset = at(0x38)?;
    let pages_map_size_comp = at(0x50)?;
    let pages_map_size_uncomp = at(0x58)?;
    let sections_map_size_comp = at(0xb0)?;
    let sections_map_id = le64(&header, 0xc0) as i64;
    let sections_map_size_uncomp = at(0xc8)?;
    let sections_map_correction = at(0xd8)?;
    let map = system_page(
        d,
        pages_map_offset
            .checked_add(0x480)
            .ok_or(DwgError::Corrupt {
                what: "the page map offset",
                at: 0x80,
            })?,
        pages_map_size_comp,
        pages_map_size_uncomp,
        pages_map_correction,
        limits,
    )?;
    let mut pages = Vec::new();
    let mut address = 0x480usize;
    let mut p = 0;
    while p + 16 <= map.len() {
        if pages.len() >= limits.max_pages {
            return Err(DwgError::TooMany {
                what: "pages",
                limit: limits.max_pages,
            });
        }
        let size = usize::try_from(le64(&map, p)).map_err(|_| DwgError::Corrupt {
            what: "a page size",
            at: p,
        })?;
        let id = le64(&map, p + 8) as i64;
        pages.push(Page { id, address, size });
        address = address.checked_add(size).ok_or(DwgError::Corrupt {
            what: "a page size",
            at: p,
        })?;
        p += 16;
    }
    let smap_page =
        pages
            .iter()
            .find(|pg| pg.id == sections_map_id)
            .ok_or(DwgError::MissingSection {
                name: "section map",
            })?;
    let sm = system_page(
        d,
        smap_page.address,
        sections_map_size_comp,
        sections_map_size_uncomp,
        sections_map_correction,
        limits,
    )?;
    let mut sections = Vec::new();
    let mut p = 0;
    while p + 0x40 <= sm.len() {
        if sections.len() >= limits.max_sections {
            return Err(DwgError::TooMany {
                what: "sections",
                limit: limits.max_sections,
            });
        }
        let size = usize::try_from(le64(&sm, p)).map_err(|_| DwgError::Corrupt {
            what: "a section size",
            at: p,
        })?;
        if size > limits.max_section_bytes {
            return Err(DwgError::TooMany {
                what: "bytes in a section",
                limit: limits.max_section_bytes,
            });
        }
        let name_length = usize::try_from(le64(&sm, p + 0x20)).map_err(|_| DwgError::Corrupt {
            what: "a section name length",
            at: p,
        })?;
        let num_pages = usize::try_from(le64(&sm, p + 0x38)).map_err(|_| DwgError::Corrupt {
            what: "a section's page count",
            at: p,
        })?;
        if num_pages > limits.max_pages || name_length > 1024 {
            return Err(DwgError::TooMany {
                what: "pages in a section",
                limit: limits.max_pages,
            });
        }
        p += 0x40;
        let name_bytes = slice(&sm, p, name_length, "a section name")?;
        let units: Vec<u16> = name_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let name = String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string();
        p += name_length;
        let mut section_pages = Vec::with_capacity(num_pages);
        for _ in 0..num_pages {
            let page = slice(&sm, p, 56, "a section's pages")?;
            let field = |o: usize| {
                usize::try_from(le64(page, o)).map_err(|_| DwgError::Corrupt {
                    what: "a section page field",
                    at: p + o,
                })
            };
            section_pages.push((field(0)?, le64(page, 16) as i64, field(24)?, field(32)?));
            p += 56;
        }
        sections.push(Described {
            name,
            size,
            pages: section_pages,
        });
    }
    Ok((pages, sections))
}

/// Assembles a data section from its pages (§5.4): Reed–Solomon coded in
/// blocks of 251 when the page on disk is that size, compressed when the
/// compressed size is smaller.
fn section(d: &[u8], pages: &[Page], s: &Described) -> Result<Vec<u8>, DwgError> {
    let mut out = vec![0u8; s.size];
    for &(offset, id, uncomp, comp) in &s.pages {
        let page = pages.iter().find(|p| p.id == id).ok_or(DwgError::Corrupt {
            what: "a section's page id",
            at: 0,
        })?;
        // §5.4: a page's data is compressed only when that is shorter, so
        // the compressed size bounds the block count.
        if offset > s.size || uncomp > s.size - offset || comp > uncomp {
            return Err(DwgError::Corrupt {
                what: "a page's sizes",
                at: page.address,
            });
        }
        let pesize = comp.div_ceil(8) * 8;
        let blocks = pesize.div_ceil(0xfb);
        let rs_page_size = (blocks * 0xff).div_ceil(32) * 32;
        let bytes = if comp != uncomp || page.size == rs_page_size {
            let raw = slice(d, page.address, page.size, "a data page")?;
            if blocks * 0xff > raw.len() + 0xff {
                return Err(DwgError::Corrupt {
                    what: "a data page smaller than its blocks",
                    at: page.address,
                });
            }
            let pedata = rs_deinterleave(raw, blocks, 0xfb);
            if comp < uncomp {
                decompress_2007(&pedata[..comp.min(pedata.len())], uncomp)?
            } else {
                slice(&pedata, 0, uncomp, "a data page")?.to_vec()
            }
        } else {
            slice(d, page.address, uncomp, "a data page")?.to_vec()
        };
        out[offset..offset + bytes.len()].copy_from_slice(&bytes);
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
    fn deinterleaving_keeps_the_data_bytes_of_each_block() {
        // Two blocks of three data bytes and one parity byte, interleaved.
        let coded = [1, 10, 2, 20, 3, 30, 99, 98];
        assert_eq!(rs_deinterleave(&coded, 2, 3), [1, 2, 3, 10, 20, 30]);
        assert_eq!(rs_deinterleave(&[7], 1, 3), [7, 0, 0]);
    }

    #[test]
    fn a_short_file_is_truncated_not_a_panic() {
        let limits = ImportLimits::default();
        assert!(matches!(
            sections(b"AC1021", &limits),
            Err(DwgError::Truncated { .. })
        ));
        let bytes = vec![0u8; 0x480];
        assert!(matches!(
            sections(&bytes, &limits),
            Err(DwgError::Corrupt { .. })
        ));
    }
}
