//! The R2007 container written (ODA spec §5): the file header coded with
//! Reed–Solomon (255, 239) and guarded by CRC-64s, data pages coded with
//! (255, 251) and compressed by the R21 compressor, the section map and
//! the page map in system pages — each twice — and the file header
//! repeated at the end. The sections are the R2004 ones.
//!
//! The layout follows what acadrust writes and AutoCAD accepts: the
//! metadata, the file header, the data pages in stream order, the section
//! map and its copy, the page map and its copy, the header copy.

use crate::Version;
use crate::write::crc64::{self, RandomEncoder};
use crate::write::lz77::compress_2007;
use crate::write::r2004::{Parts, section_data};
use crate::write::rs::{self, DATA_K, Field, SYSTEM_K};

const METADATA_SIZE: usize = 0x110;
/// The metadata block and the file header page before the first page.
const RESERVED: usize = 0x480;
const FILE_HEADER_SIZE: usize = 0x400;
const CHECK_DATA_SIZE: usize = 0x28;
const MIN_SYSTEM_PAGE: usize = 0x400;
/// The seed of the CRC random encoder and of the CRC-64s: a fixed one,
/// so the output is deterministic (§5.2.1.1).
const SEED: u64 = 0;

/// What the section map says of a section (§5.5): its hash, its page
/// size, and whether its pages are compressed (4) or stored (1).
struct Kind {
    name: &'static str,
    hash: u64,
    page_size: usize,
    encoding: u64,
}

const KINDS: &[Kind] = &[
    Kind {
        name: "AcDb:Security",
        hash: 0x4a02_04ea,
        page_size: 0xf800,
        encoding: 1,
    },
    Kind {
        name: "AcDb:FileDepList",
        hash: 0x6c42_05ca,
        page_size: 0x100,
        encoding: 1,
    },
    Kind {
        name: "AcDb:VBAProject",
        hash: 0x586e_0544,
        page_size: 0x400,
        encoding: 1,
    },
    Kind {
        name: "AcDb:AppInfo",
        hash: 0x3fa0_043e,
        page_size: 0x300,
        encoding: 1,
    },
    Kind {
        name: "AcDb:Preview",
        hash: 0x40aa_0473,
        page_size: 0x400,
        encoding: 1,
    },
    Kind {
        name: "AcDb:SummaryInfo",
        hash: 0x717a_060f,
        page_size: 0x80,
        encoding: 1,
    },
    Kind {
        name: "AcDb:RevHistory",
        hash: 0x60a2_05b3,
        page_size: 0x1000,
        encoding: 4,
    },
    Kind {
        name: "AcDb:AcDbObjects",
        hash: 0x674c_05a9,
        page_size: 0xf800,
        encoding: 4,
    },
    Kind {
        name: "AcDb:ObjFreeSpace",
        hash: 0x77e2_061f,
        page_size: 0xf800,
        encoding: 4,
    },
    Kind {
        name: "AcDb:Template",
        hash: 0x4a14_04ce,
        page_size: 0x400,
        encoding: 4,
    },
    Kind {
        name: "AcDb:Handles",
        hash: 0x3f6e_0450,
        page_size: 0xf800,
        encoding: 4,
    },
    Kind {
        name: "AcDb:Classes",
        hash: 0x3f54_045f,
        page_size: 0xf800,
        encoding: 4,
    },
    Kind {
        name: "AcDb:AuxHeader",
        hash: 0x54f0_050a,
        page_size: 0x800,
        encoding: 4,
    },
    Kind {
        name: "AcDb:Header",
        hash: 0x32b8_03d9,
        page_size: 0x800,
        encoding: 4,
    },
];

/// A page as placed in the file, in file order (§5.3).
struct Placed {
    id: i64,
    size: usize,
}

/// A data page's row of the section map (§5.5).
struct SectionPage {
    offset: u64,
    id: i64,
    uncompressed: usize,
    compressed: usize,
    checksum: u32,
    crc: u64,
}

/// A section as the map describes it.
struct Described {
    kind: &'static Kind,
    size: usize,
    pages: Vec<SectionPage>,
}

/// A system page encoded: the bytes, and what the file header says of
/// it.
struct SystemPage {
    bytes: Vec<u8>,
    compressed: usize,
    uncompressed: usize,
    crc_compressed: u64,
    crc_uncompressed: u64,
    correction: usize,
}

fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

fn align32(n: usize) -> usize {
    n.div_ceil(0x20) * 0x20
}

/// The size of a system page for `data` bytes (§5.3.1): the data at
/// least twice, with the parity, never below 0x400.
fn system_page_size(data: usize) -> usize {
    let blocks = (align8(data) * 2).div_ceil(SYSTEM_K);
    align32(blocks * 255).max(MIN_SYSTEM_PAGE)
}

/// Assembles the file.
pub fn file(parts: &Parts) -> Vec<u8> {
    let system = Field::new(rs::SYSTEM_POLY);
    let data_field = Field::new(rs::DATA_POLY);
    let mut out: Vec<u8> = vec![0; RESERVED];
    let mut placed: Vec<Placed> = Vec::new();
    let mut next_id: i64 = 1;

    // The data pages, in stream order.
    let mut described: Vec<Described> = Vec::new();
    for (name, data) in section_data(parts, Version::R2007) {
        let kind = KINDS
            .iter()
            .find(|k| k.name == name)
            .expect("a known section");
        let mut pages = Vec::new();
        let count = data.len().div_ceil(kind.page_size).max(1);
        for k in 0..count {
            let start = k * kind.page_size;
            let end = (start + kind.page_size).min(data.len());
            let page = &data[start.min(end)..end];
            let checksum = crc64::page_checksum(0, page);
            let (bytes, compressed, crc) = if kind.encoding == 1 {
                let bytes = rs::data_then_parity(&data_field, page, DATA_K);
                (bytes, page.len(), crc64::mirrored(SEED, page))
            } else {
                let packed = compress_2007(page);
                let stored: &[u8] = if packed.len() < page.len() {
                    &packed
                } else {
                    page
                };
                let blocks = align8(stored.len()).div_ceil(DATA_K).max(1);
                let bytes = rs::interleaved(&data_field, stored, blocks, DATA_K);
                (bytes, stored.len(), crc64::mirrored(SEED, stored))
            };
            let mut size = align32(bytes.len());
            if kind.encoding == 1 {
                // A stored page is read raw unless its slot is the size a
                // coded one would take (§5.4): keep the two apart.
                let blocks = align8(page.len()).div_ceil(DATA_K).max(1);
                if size == align32(blocks * 255) {
                    size += 0x20;
                }
            }
            let id = next_id;
            next_id += 1;
            out.extend_from_slice(&bytes);
            out.resize(out.len() + size - bytes.len(), 0);
            placed.push(Placed { id, size });
            pages.push(SectionPage {
                offset: start as u64,
                id,
                uncompressed: page.len(),
                compressed,
                checksum,
                crc,
            });
        }
        described.push(Described {
            kind,
            size: data.len(),
            pages,
        });
    }

    // The section map and its copy (§5.5), in the map's own order.
    let mut map = Vec::new();
    for kind in KINDS {
        let Some(section) = described.iter().find(|s| std::ptr::eq(s.kind, kind)) else {
            continue;
        };
        let units: Vec<u16> = kind.name.encode_utf16().chain(std::iter::once(0)).collect();
        for value in [
            section.size as u64,
            kind.page_size as u64,
            0, // not encrypted
            kind.hash,
            (units.len() * 2) as u64,
            0,
            kind.encoding,
            section.pages.len() as u64,
        ] {
            map.extend_from_slice(&value.to_le_bytes());
        }
        for unit in &units {
            map.extend_from_slice(&unit.to_le_bytes());
        }
        for page in &section.pages {
            for value in [
                page.offset,
                kind.page_size as u64,
                page.id as u64,
                page.uncompressed as u64,
                page.compressed as u64,
                u64::from(page.checksum),
                page.crc,
            ] {
                map.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    let section_map = system_page(&system, &map, system_page_size(map.len()));
    let section_map_size = align32(section_map.bytes.len()).max(system_page_size(map.len()));
    let section_map_id = next_id;
    let section_map2_id = next_id + 1;
    next_id += 2;
    for id in [section_map_id, section_map2_id] {
        out.extend_from_slice(&section_map.bytes);
        out.resize(out.len() + section_map_size - section_map.bytes.len(), 0);
        placed.push(Placed {
            id,
            size: section_map_size,
        });
    }

    // The page map and its copy (§5.3): every page's size and id in file
    // order, the map's own two pages included, then a zero pair.
    let page_map_id = next_id;
    let page_map2_id = next_id + 1;
    next_id += 2;
    let page_map_size = system_page_size((placed.len() + 3) * 16);
    let mut pages = Vec::new();
    for page in &placed {
        pages.extend_from_slice(&(page.size as u64).to_le_bytes());
        pages.extend_from_slice(&page.id.to_le_bytes());
    }
    for id in [page_map_id, page_map2_id] {
        pages.extend_from_slice(&(page_map_size as u64).to_le_bytes());
        pages.extend_from_slice(&id.to_le_bytes());
    }
    pages.extend_from_slice(&[0u8; 16]);
    let page_map = system_page(&system, &pages, page_map_size);
    let page_map_size = align32(page_map.bytes.len()).max(page_map_size);
    let page_map_at = out.len();
    for _ in 0..2 {
        out.extend_from_slice(&page_map.bytes);
        out.resize(out.len() + page_map_size - page_map.bytes.len(), 0);
    }
    let page_map2_at = page_map_at + page_map_size;
    let header2_at = out.len();
    let file_size = header2_at + FILE_HEADER_SIZE;

    // The metadata (§5.2), the CRC random encoder drawn on in the order
    // the specification fixes (§5.2.1.1).
    let mut rng = RandomEncoder::new(SEED);
    let sections_map_crc_seed = rng.encode(SEED as u32);
    let pages_map_crc_seed = rng.encode(SEED as u32);
    let random1 = rng.next_u64();
    let random2 = rng.next_u64();
    let check_encoded_seed = rng.encode(SEED as u32);
    let check_normal = {
        let mut v = [0u64; 8];
        v[0] = crc64::rotate(random1, random2);
        v[1] = crc64::rotate(v[0], v[0]);
        v[2] = crc64::rotate(random2, v[1]);
        v[3] = crc64::rotate(v[2], v[2]);
        v[4] = crc64::rotate(random1, v[3]);
        v[5] = crc64::rotate(v[4], v[4]);
        v[6] = crc64::rotate(v[5], v[5]);
        v[7] = crc64::rotate(v[6], v[6]);
        crc64::check_normal(random2, &longs(&v))
    };
    let check_mirrored = {
        let mut v = [0u64; 8];
        v[0] = crc64::rotate(random1, random2);
        v[1] = crc64::rotate(check_normal, v[0]);
        v[2] = crc64::rotate(random2, v[1]);
        v[3] = crc64::rotate(check_normal, v[2]);
        v[4] = crc64::rotate(random1, v[3]);
        v[5] = crc64::rotate(check_normal, v[4]);
        v[6] = crc64::rotate(random2, v[5]);
        v[7] = crc64::rotate(v[6], v[6]);
        crc64::check_mirrored(random1, &longs(&v))
    };
    let check_data = longs(&[
        check_normal,
        check_mirrored,
        random1,
        random2,
        check_encoded_seed,
    ]);
    let crc_seed_encoded = rng.encode(SEED as u32);
    let mut metadata = longs(&[
        0x70,
        file_size as u64,
        page_map.crc_compressed,
        page_map.correction as u64,
        pages_map_crc_seed,
        (page_map2_at - RESERVED) as u64,
        page_map2_id as u64,
        (page_map_at - RESERVED) as u64,
        page_map_id as u64,
        (header2_at - RESERVED) as u64,
        page_map.compressed as u64,
        page_map.uncompressed as u64,
        (placed.len() + 2) as u64,
        (next_id - 1) as u64,
        0x20,
        0x40,
        page_map.crc_uncompressed,
        0xf800,
        4,
        1,
        (described.len() + 1) as u64,
        section_map.crc_uncompressed,
        section_map.compressed as u64,
        section_map2_id as u64,
        section_map_id as u64,
        section_map.uncompressed as u64,
        section_map.crc_compressed,
        section_map.correction as u64,
        sections_map_crc_seed,
        0x60100,
        SEED,
        crc_seed_encoded,
        SEED,
        0, // the CRC, over the rest
    ]);
    debug_assert_eq!(metadata.len(), METADATA_SIZE);
    let crc = crc64::normal(0, &metadata);
    metadata[0x108..].copy_from_slice(&crc.to_le_bytes());
    let header = file_header(&metadata, &mut rng, &system, &check_data);
    out[0x80..RESERVED].copy_from_slice(&header);
    out.extend_from_slice(&header);

    // The metadata block (§5.1): the version, the addresses of the
    // preview, the summary, and the application info.
    let first_page_at = |name: &str| -> u32 {
        let mut address = RESERVED;
        for page in &placed {
            if described
                .iter()
                .any(|s| s.kind.name == name && s.pages.first().is_some_and(|p| p.id == page.id))
            {
                return address as u32;
            }
            address += page.size;
        }
        0
    };
    out[..6].copy_from_slice(b"AC1021");
    out[0x0b] = 0x19;
    out[0x0c] = 0x03;
    out[0x0d..0x11].copy_from_slice(&first_page_at("AcDb:Preview").to_le_bytes());
    out[0x11] = 0x1b;
    out[0x12] = 0x19;
    out[0x13..0x15].copy_from_slice(&30u16.to_le_bytes());
    out[0x20..0x24].copy_from_slice(&first_page_at("AcDb:SummaryInfo").to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&0x80u32.to_le_bytes());
    out[0x2c..0x30].copy_from_slice(&first_page_at("AcDb:AppInfo").to_le_bytes());
    out
}

/// Longs as little-endian bytes.
fn longs(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A system page (§5.3): the data compressed when that is shorter,
/// repeated as many times as fill the page's blocks of 239 — the
/// correction factor, two at least — and Reed–Solomon coded.
fn system_page(field: &Field, data: &[u8], target: usize) -> SystemPage {
    let crc_uncompressed = crc64::mirrored(SEED, data);
    let packed = compress_2007(data);
    let stored: &[u8] = if packed.len() < data.len() {
        &packed
    } else {
        data
    };
    let crc_compressed = crc64::mirrored(SEED, stored);
    let aligned = align8(stored.len());
    let correction = ((target / 255) * SYSTEM_K)
        .checked_div(aligned)
        .map_or(1, |c| c.max(2));
    let total = aligned * correction;
    let blocks = total.div_ceil(SYSTEM_K).max(1);
    let mut repeated = vec![0u8; blocks * SYSTEM_K];
    if aligned != 0 {
        for copy in repeated[..total].chunks_mut(aligned) {
            copy[..stored.len()].copy_from_slice(stored);
        }
    }
    SystemPage {
        bytes: rs::interleaved(field, &repeated, blocks, SYSTEM_K),
        compressed: stored.len(),
        uncompressed: data.len(),
        crc_compressed,
        crc_uncompressed,
        correction,
    }
}

/// The file header page (§5.2.1): the metadata compressed behind the
/// checking sequence and its CRCs, the block repeated through three
/// codewords of 239, Reed–Solomon coded, random padding, and the check
/// data in the last 0x28 bytes.
fn file_header(
    metadata: &[u8],
    rng: &mut RandomEncoder,
    field: &Field,
    check_data: &[u8],
) -> Vec<u8> {
    let packed = compress_2007(metadata);
    let (stored, length): (&[u8], i32) = if packed.len() < metadata.len() {
        (&packed, packed.len() as i32)
    } else {
        (metadata, -(metadata.len() as i32))
    };
    let crc = crc64::normal(0, stored);
    let value1 = rng.next_u64();
    let value2 = crc64::rotate(value1, value1);
    let sequence_crc = crc64::normal_seed1(0, &longs(&[value1, value2]));
    let mut block = Vec::with_capacity(32 + stored.len() + 8);
    block.extend_from_slice(&sequence_crc.to_le_bytes());
    block.extend_from_slice(&value1.to_le_bytes());
    block.extend_from_slice(&crc.to_le_bytes());
    block.extend_from_slice(&length.to_le_bytes());
    block.extend_from_slice(&0i32.to_le_bytes());
    block.extend_from_slice(stored);
    let padded = align8(block.len());
    if block.len() < padded {
        let mut pad = vec![0u8; padded - block.len()];
        rng.fill(&mut pad);
        block.extend_from_slice(&pad);
    }
    let total = 3 * SYSTEM_K;
    let mut before = vec![0u8; total];
    let mut at = 0;
    while at + block.len() <= total {
        before[at..at + block.len()].copy_from_slice(&block);
        at += block.len();
    }
    if at < total {
        rng.fill(&mut before[at..]);
    }
    let coded = rs::interleaved(field, &before, 3, SYSTEM_K);
    let mut page = vec![0u8; FILE_HEADER_SIZE];
    let rs_end = FILE_HEADER_SIZE - CHECK_DATA_SIZE;
    page[..coded.len()].copy_from_slice(&coded);
    rng.fill(&mut page[coded.len()..rs_end]);
    page[rs_end..].copy_from_slice(check_data);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_page_fits_its_data_twice_and_is_read_back() {
        let field = Field::new(rs::SYSTEM_POLY);
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7) as u8).collect();
        let target = system_page_size(data.len());
        assert_eq!(target, MIN_SYSTEM_PAGE);
        let page = system_page(&field, &data, target);
        assert!(page.correction >= 2);
        assert_eq!(page.uncompressed, 300);
        // The reader's view: the blocks it de-interleaves hold the data.
        let pesize = align8(page.compressed) * page.correction;
        let blocks = pesize.div_ceil(SYSTEM_K);
        assert!(blocks * 255 <= target);
        let back = crate::r2007::rs_deinterleave(&page.bytes, blocks, SYSTEM_K);
        let stored = if page.compressed < page.uncompressed {
            crate::lz77::decompress_2007(&back[..page.compressed], page.uncompressed).unwrap()
        } else {
            back[..page.uncompressed].to_vec()
        };
        assert_eq!(stored, data);
    }

    #[test]
    fn the_file_header_page_decodes_to_its_metadata() {
        let field = Field::new(rs::SYSTEM_POLY);
        let metadata: Vec<u8> = (0..METADATA_SIZE as u32).map(|i| (i * 3) as u8).collect();
        let mut rng = RandomEncoder::new(SEED);
        let check = vec![0xAAu8; CHECK_DATA_SIZE];
        let page = file_header(&metadata, &mut rng, &field, &check);
        assert_eq!(page.len(), FILE_HEADER_SIZE);
        assert_eq!(&page[FILE_HEADER_SIZE - CHECK_DATA_SIZE..], &check[..]);
        let pedata = crate::r2007::rs_deinterleave(&page[..0x3d8], 3, SYSTEM_K);
        let length = i32::from_le_bytes(pedata[24..28].try_into().unwrap());
        let back = if length > 0 {
            crate::lz77::decompress_2007(&pedata[32..32 + length as usize], METADATA_SIZE).unwrap()
        } else {
            pedata[32..32 + METADATA_SIZE].to_vec()
        };
        assert_eq!(back, metadata);
    }
}
