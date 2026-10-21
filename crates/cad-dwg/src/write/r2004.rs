//! The R2004 container written (ODA spec §4): the encrypted file header,
//! the data sections cut into pages with masked headers and checksums,
//! the section map and the page map in system pages, and the header
//! repeated at the end. The sections are the R2000 ones in a different
//! wrapping, plus the ones every R2004 reader expects to find.

use crate::Version;
use crate::crc::{checksum_2004, crc32};
use crate::r2004::magic_sequence;
use crate::write::lz77::compress_2004;
use crate::write::objects::ObjectBytes;
use crate::write::r2000::{CODEPAGE, aux_header, obj_free_space, object_map, preview};

const SYSTEM_PAGE_MAP: u32 = 0x4163_0e3b;
const SYSTEM_SECTION_MAP: u32 = 0x4163_003b;
const DATA_PAGE: u32 = 0x4163_043b;
const PAGE_SIZE: usize = 0x7400;

/// What the container needs besides the objects.
pub struct Parts {
    pub header: Vec<u8>,
    pub classes: Vec<u8>,
    /// In handle order; the map lists them in this order.
    pub objects: Vec<ObjectBytes>,
    pub handseed: u64,
    /// 1 for metric drawings, 0 otherwise (§22).
    pub measurement: u16,
}

/// A data section to write: its name, its bytes, how it is paged.
struct DataSection {
    name: &'static str,
    data: Vec<u8>,
    page_size: usize,
    compressed: bool,
}

/// A page as placed in the file: its number and physical size; the page
/// map gives the addresses by adding the sizes up.
struct Placed {
    number: u32,
    size: usize,
}

/// Rounds a length up to the 0x20 boundary pages sit on.
fn aligned(len: usize) -> usize {
    len.div_ceil(0x20) * 0x20
}

/// The data sections of an R2004 or R2007 file, in the order of the
/// stream (§4.5, §5.5): the name and the bytes of each.
pub(super) fn section_data(parts: &Parts, version: Version) -> Vec<(&'static str, Vec<u8>)> {
    // The objects, then the map of their offsets within the section.
    let mut objects = Vec::new();
    let mut entries: Vec<(u64, usize)> = Vec::with_capacity(parts.objects.len());
    for object in &parts.objects {
        entries.push((object.handle, objects.len()));
        objects.extend_from_slice(&object.bytes);
    }
    let handles = object_map(&entries);
    let mut template = Vec::new();
    template.extend_from_slice(&0u16.to_le_bytes());
    template.extend_from_slice(&parts.measurement.to_le_bytes());
    vec![
        ("AcDb:SummaryInfo", summary_info(version)),
        ("AcDb:Preview", preview()),
        ("AcDb:AppInfo", app_info()),
        ("AcDb:FileDepList", vec![0; 8]),
        ("AcDb:RevHistory", rev_history()),
        ("AcDb:AcDbObjects", objects),
        ("AcDb:ObjFreeSpace", obj_free_space(parts.objects.len(), 0)),
        ("AcDb:Template", template),
        ("AcDb:Handles", handles),
        ("AcDb:Classes", parts.classes.clone()),
        ("AcDb:AuxHeader", aux_header(parts.handseed, version)),
        ("AcDb:Header", parts.header.clone()),
    ]
}

/// Assembles the file.
pub fn file(parts: &Parts) -> Vec<u8> {
    // The sections in the order of the stream (§4.5): the small fixed ones
    // in small uncompressed pages, the rest compressed.
    let sections: Vec<DataSection> = section_data(parts, Version::R2004)
        .into_iter()
        .map(|(name, data)| {
            let (page_size, compressed) = match name {
                "AcDb:SummaryInfo" => (0x100, false),
                "AcDb:Preview" => (0x400, false),
                "AcDb:AppInfo" | "AcDb:FileDepList" => (0x80, false),
                _ => (PAGE_SIZE, true),
            };
            DataSection {
                name,
                data,
                page_size,
                compressed,
            }
        })
        .collect();

    // The file header's 0x100 bytes come first; every page after it is
    // aligned to 0x20 and numbered from one in the order written.
    let mut out: Vec<u8> = vec![0; 0x100];
    let mut pages: Vec<Placed> = Vec::new();
    // Per section: its pages as (number, stored size, start offset).
    let mut section_pages: Vec<Vec<(u32, u32, u64)>> = Vec::new();
    let mut preview_at = 0u32;
    let mut summary_at = 0u32;
    for (id, section) in sections.iter().enumerate() {
        let mut listed = Vec::new();
        let count = section.data.len().div_ceil(section.page_size).max(1);
        for k in 0..count {
            let start = k * section.page_size;
            let mut page = vec![0u8; section.page_size];
            let end = (start + section.page_size).min(section.data.len());
            if start < end {
                page[..end - start].copy_from_slice(&section.data[start..end]);
            }
            let payload = if section.compressed {
                compress_2004(&page)
            } else {
                page
            };
            let number = pages.len() as u32 + 1;
            let address = out.len();
            let size = aligned(32 + payload.len());
            if section.name == "AcDb:Preview" && k == 0 {
                preview_at = address as u32 + 32;
            }
            if section.name == "AcDb:SummaryInfo" && k == 0 {
                summary_at = address as u32 + 32;
            }
            // The masked header (§4.6): the checksums first, over the
            // data and over the header with its own field zero.
            let data_checksum = checksum_2004(0, &payload);
            let mut header = [0u8; 32];
            let fields: [u32; 8] = [
                DATA_PAGE,
                id as u32,
                payload.len() as u32,
                size as u32,
                start as u32,
                0,
                0,
                data_checksum,
            ];
            for (i, field) in fields.iter().enumerate() {
                header[i * 4..i * 4 + 4].copy_from_slice(&field.to_le_bytes());
            }
            let header_checksum = checksum_2004(data_checksum, &header);
            header[0x18..0x1c].copy_from_slice(&header_checksum.to_le_bytes());
            let mask = 0x4164_536bu32 ^ address as u32;
            for i in 0..8 {
                let v =
                    u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().expect("four")) ^ mask;
                header[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&header);
            out.extend_from_slice(&payload);
            out.resize(address + size, 0);
            pages.push(Placed { number, size });
            listed.push((number, payload.len() as u32, start as u64));
        }
        section_pages.push(listed);
    }

    // The section map (§4.5), a system page.
    let mut map = Vec::new();
    let count = sections.len() as u32;
    for value in [count, 2, PAGE_SIZE as u32, 0, count] {
        map.extend_from_slice(&value.to_le_bytes());
    }
    for (id, section) in sections.iter().enumerate() {
        let listed = &section_pages[id];
        map.extend_from_slice(&(section.data.len() as u64).to_le_bytes());
        map.extend_from_slice(&(listed.len() as u32).to_le_bytes());
        map.extend_from_slice(&(section.page_size as u32).to_le_bytes());
        map.extend_from_slice(&0u32.to_le_bytes());
        map.extend_from_slice(&(if section.compressed { 2u32 } else { 1 }).to_le_bytes());
        map.extend_from_slice(&(id as u32).to_le_bytes());
        map.extend_from_slice(&0u32.to_le_bytes());
        let mut name = [0u8; 64];
        name[..section.name.len()].copy_from_slice(section.name.as_bytes());
        map.extend_from_slice(&name);
        for &(number, stored, start) in listed {
            map.extend_from_slice(&number.to_le_bytes());
            map.extend_from_slice(&stored.to_le_bytes());
            map.extend_from_slice(&start.to_le_bytes());
        }
    }
    let section_map_id = pages.len() as u32 + 1;
    let section_map_size = system_page(&mut out, SYSTEM_SECTION_MAP, &map);
    pages.push(Placed {
        number: section_map_id,
        size: section_map_size,
    });

    // The page map (§4.4), the last page: every page's number and size,
    // itself included.
    let page_map_id = section_map_id + 1;
    let page_map_at = out.len();
    let mut page_map = Vec::new();
    for page in &pages {
        page_map.extend_from_slice(&page.number.to_le_bytes());
        page_map.extend_from_slice(&(page.size as u32).to_le_bytes());
    }
    // Its own entry: the size is known once written, so write it with a
    // guess and again with the size the guess gave.
    let mut own_size = 0x40;
    let page_map_size = loop {
        let mut attempt = page_map.clone();
        attempt.extend_from_slice(&page_map_id.to_le_bytes());
        attempt.extend_from_slice(&(own_size as u32).to_le_bytes());
        let mut trial = Vec::new();
        let size = system_page(&mut trial, SYSTEM_PAGE_MAP, &attempt);
        if size == own_size {
            out.extend_from_slice(&trial);
            break size;
        }
        own_size = size;
    };
    let last_end = page_map_at + page_map_size;

    // The header repeated at the end, after a blank system page header
    // (§4.1), as the ODA lays it out.
    let second_header_at = out.len() + 20;
    out.extend_from_slice(&SYSTEM_PAGE_MAP.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);

    // The file header (§4.1).
    out[..6].copy_from_slice(b"AC1018");
    out[0x0b] = 0;
    out[0x0c] = 1;
    out[0x0d..0x11].copy_from_slice(&preview_at.to_le_bytes());
    out[0x11] = 0x1b;
    out[0x12] = 0x19;
    out[0x13..0x15].copy_from_slice(&CODEPAGE.to_le_bytes());
    out[0x20..0x24].copy_from_slice(&summary_at.to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&0x80u32.to_le_bytes());
    let mut header = [0u8; 0x6c];
    header[..12].copy_from_slice(b"AcFssFcAJMB\0");
    let put32 =
        |h: &mut [u8; 0x6c], at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let put64 =
        |h: &mut [u8; 0x6c], at: usize, v: u64| h[at..at + 8].copy_from_slice(&v.to_le_bytes());
    put32(&mut header, 0x10, 0x6c);
    put32(&mut header, 0x14, 4);
    put32(&mut header, 0x24, 1);
    put32(&mut header, 0x28, page_map_id);
    put64(&mut header, 0x2c, (last_end - 0x100) as u64);
    put64(&mut header, 0x34, second_header_at as u64);
    put32(&mut header, 0x40, pages.len() as u32 + 1);
    put32(&mut header, 0x44, 0x20);
    put32(&mut header, 0x48, 0x80);
    put32(&mut header, 0x4c, 0x40);
    put32(&mut header, 0x50, page_map_id);
    put64(&mut header, 0x54, (page_map_at - 0x100) as u64);
    put32(&mut header, 0x5c, section_map_id);
    put32(&mut header, 0x60, pages.len() as u32 + 1);
    let crc = crc32(0, &header);
    put32(&mut header, 0x68, crc);
    let magic = magic_sequence(0x100);
    let encrypted: Vec<u8> = header.iter().zip(&magic).map(|(a, b)| a ^ b).collect();
    out[0x80..0xec].copy_from_slice(&encrypted);
    out[0xec..0x100].copy_from_slice(&magic[0xec..0x100]);
    out.extend_from_slice(&encrypted);
    out
}

/// A system page (§4.3): the header, the data compressed, the page padded
/// to the 0x20 boundary. Returns the page's size.
fn system_page(out: &mut Vec<u8>, kind: u32, data: &[u8]) -> usize {
    let payload = compress_2004(data);
    let mut header = [0u8; 0x14];
    header[..4].copy_from_slice(&kind.to_le_bytes());
    header[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
    header[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[12..16].copy_from_slice(&2u32.to_le_bytes());
    let seed = checksum_2004(0, &header);
    let checksum = checksum_2004(seed, &payload);
    header[16..20].copy_from_slice(&checksum.to_le_bytes());
    let at = out.len();
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload);
    let size = aligned(0x14 + payload.len());
    out.resize(at + size, 0);
    size
}

/// A text of the summary and app info sections: a short count of UTF-16
/// units, the terminator counted, then the units.
pub(super) fn text16(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

/// AcDb:SummaryInfo: eight empty properties as short-counted texts — a
/// single byte each before R2007, a UTF-16 unit from it — three zero
/// dates, no custom properties, two zero longs.
pub(super) fn summary_info(version: Version) -> Vec<u8> {
    let mut out = Vec::with_capacity(66);
    for _ in 0..8 {
        out.extend_from_slice(&1u16.to_le_bytes());
        out.push(0);
        if version >= Version::R2007 {
            out.push(0);
        }
    }
    out.extend_from_slice(&[0u8; 24]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out
}

/// AcDb:AppInfo: the writer named, in the form AutoCAD 2004 writes it.
pub(super) fn app_info() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&2u32.to_le_bytes());
    text16(&mut out, "AppInfoDataList");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    text16(&mut out, env!("CARGO_PKG_VERSION"));
    out.extend_from_slice(&[0u8; 16]);
    text16(&mut out, "Written by CadSlate2D");
    out.extend_from_slice(&[0u8; 16]);
    text16(
        &mut out,
        &format!(
            "<ProductInformation name=\"CadSlate2D\" build_version=\"{v}\" registry_version=\"{v}\" install_id_string=\"CadSlate2D\" registry_localeID=\"1033\"/>",
            v = env!("CARGO_PKG_VERSION")
        ),
    );
    out
}

/// AcDb:RevHistory: version zero, one history entry of zero.
pub(super) fn rev_history() -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    for value in [0u32, 0, 1, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_sections_have_the_sizes_the_specifications_give() {
        assert_eq!(summary_info(Version::R2004).len(), 58);
        assert_eq!(summary_info(Version::R2007).len(), 66);
        assert_eq!(rev_history().len(), 16);
        assert!(app_info().starts_with(&[2, 0, 0, 0, 16, 0, b'A', 0, b'p', 0]));
    }

    #[test]
    fn a_system_page_is_aligned_and_checked() {
        let mut out = Vec::new();
        let size = system_page(&mut out, SYSTEM_PAGE_MAP, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(size % 0x20, 0);
        assert_eq!(out.len(), size);
        let stored = u32::from_le_bytes(out[16..20].try_into().unwrap());
        let mut zeroed = out[..0x14].to_vec();
        zeroed[16..20].fill(0);
        let compressed = u32::from_le_bytes(out[8..12].try_into().unwrap()) as usize;
        let seed = checksum_2004(0, &zeroed);
        assert_eq!(checksum_2004(seed, &out[0x14..0x14 + compressed]), stored);
    }
}
