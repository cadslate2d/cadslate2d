//! The R2000 container written (ODA spec §3, §14, §21, §22, §23.1, §27):
//! the file header with its six section locators, the header and classes
//! sections, the objects, the object map, the free-space and template
//! sections, the auxiliary header, and an empty preview at the end.

use crate::crc::crc16;
use crate::r2000::HEADER_SENTINEL;
use crate::write::bits::BitWriter;
use crate::write::objects::ObjectBytes;

const PREVIEW_BEGIN: [u8; 16] = [
    0x1F, 0x25, 0x6D, 0x07, 0xD4, 0x36, 0x28, 0x28, 0x9D, 0x57, 0xCA, 0x3F, 0x9D, 0x44, 0x10, 0x2B,
];
const PREVIEW_END: [u8; 16] = [
    0xE0, 0xDA, 0x92, 0xF8, 0x2B, 0xC9, 0xD7, 0xD7, 0x62, 0xA8, 0x35, 0xC0, 0x62, 0xBB, 0xEF, 0xD4,
];

/// The code page of the files this crate writes: ANSI 1252, as the DXF of
/// this release declares.
pub(super) const CODEPAGE: u16 = 30;

/// An empty preview (§14.2): the sentinels around a size of one and no
/// images.
pub(super) fn preview() -> Vec<u8> {
    let mut out = Vec::with_capacity(53);
    out.extend_from_slice(&PREVIEW_BEGIN);
    // No images: the count, then sixteen bytes of padding counted in the
    // size, so a reader that takes the sentinels off and reads the size
    // where a real image's would be (LibreDWG, R2007) still finds it.
    out.extend_from_slice(&17u32.to_le_bytes());
    out.push(0);
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&PREVIEW_END);
    out
}

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

/// Assembles the file.
pub fn file(parts: &Parts) -> Vec<u8> {
    // The file header: six locators after the fixed fields (§3.2).
    let locators = 6;
    let header_len = 0x19 + 9 * locators + 2 + 16;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"AC1015");
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 1]);
    out.extend_from_slice(&0u32.to_le_bytes()); // the preview address, patched
    out.extend_from_slice(&[0x1b, 0x19]);
    out.extend_from_slice(&CODEPAGE.to_le_bytes());
    out.extend_from_slice(&(locators as u32).to_le_bytes());
    out.resize(header_len, 0);

    let mut addresses: [(u32, u32); 6] = [(0, 0); 6];
    let place = |out: &mut Vec<u8>, bytes: &[u8]| -> (u32, u32) {
        let at = out.len() as u32;
        out.extend_from_slice(bytes);
        (at, bytes.len() as u32)
    };
    addresses[0] = place(&mut out, &parts.header);
    addresses[1] = place(&mut out, &parts.classes);

    // The objects, then the map of their addresses (§23.1).
    let objects_at = out.len();
    let mut entries: Vec<(u64, usize)> = Vec::with_capacity(parts.objects.len());
    for object in &parts.objects {
        entries.push((object.handle, out.len()));
        out.extend_from_slice(&object.bytes);
    }
    let map = object_map(&entries);
    addresses[2] = place(&mut out, &map);

    addresses[3] = place(&mut out, &obj_free_space(parts.objects.len(), objects_at));
    let mut template = Vec::new();
    template.extend_from_slice(&0u16.to_le_bytes());
    template.extend_from_slice(&parts.measurement.to_le_bytes());
    addresses[4] = place(&mut out, &template);
    addresses[5] = place(&mut out, &aux_header(parts.handseed, crate::Version::R2000));

    // The preview, empty, at the end; the file header points at it.
    let preview_at = out.len() as u32;
    out.extend_from_slice(&preview());
    out[0x0d..0x11].copy_from_slice(&preview_at.to_le_bytes());

    // The locators, then the header's CRC and sentinel (§3.2.6).
    for (i, (address, size)) in addresses.iter().enumerate() {
        let at = 0x19 + 9 * i;
        out[at] = i as u8;
        out[at + 1..at + 5].copy_from_slice(&address.to_le_bytes());
        out[at + 5..at + 9].copy_from_slice(&size.to_le_bytes());
    }
    let end = 0x19 + 9 * locators;
    let crc = crc16(0, &out[..end]) ^ 0x8461;
    out[end..end + 2].copy_from_slice(&crc.to_le_bytes());
    out[end + 2..end + 18].copy_from_slice(&HEADER_SENTINEL);
    out
}

/// The object map (§23.1): sections of at most 2032 bytes of handle and
/// address offsets, each with its big-endian size and CRC, then the
/// empty section that ends it. The addresses are the file's in R2000, the
/// objects section's from R2004.
pub(super) fn object_map(entries: &[(u64, usize)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut section = BitWriter::new();
    let mut last_handle = 0u64;
    let mut last_address = 0i64;
    let close = |out: &mut Vec<u8>, section: &mut BitWriter| {
        let body = std::mem::take(section).into_bytes();
        let size = (body.len() + 2) as u16;
        let start = out.len();
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(&body);
        let crc = crc16(0xC0C1, &out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    for &(handle, address) in entries {
        section.umc(handle.wrapping_sub(last_handle));
        section.mc(address as i64 - last_address);
        last_handle = handle;
        last_address = address as i64;
        if section.len() > 2020 {
            close(&mut out, &mut section);
            last_handle = 0;
            last_address = 0;
        }
    }
    if !section.is_empty() {
        close(&mut out, &mut section);
    }
    close(&mut out, &mut BitWriter::new());
    out
}

/// The ObjFreeSpace section (§21.1): counts and the values the ODA writes.
pub(super) fn obj_free_space(objects: usize, objects_at: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(53);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(objects as u32).to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // TDUPDATE
    out.extend_from_slice(&(objects_at as u32).to_le_bytes());
    out.push(4);
    for value in [0x32u32, 0, 0x64, 0, 0x200, 0, 0xffff_ffff, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// The auxiliary header (§27): the version twice, the counts of saves, the
/// handle seed; the dates zero.
pub(super) fn aux_header(handseed: u64, version: crate::Version) -> Vec<u8> {
    let number = u16::from(version.number());
    let mut out = Vec::with_capacity(123);
    out.extend_from_slice(&[0xff, 0x77, 0x01]);
    out.extend_from_slice(&number.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // the maintenance version
    out.extend_from_slice(&1u32.to_le_bytes()); // saves
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for value in [number, 0, number, 0, 5, 0x893, 5, 0x893, 0, 1] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for _ in 0..5 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(&0u64.to_le_bytes()); // TDCREATE
    out.extend_from_slice(&0u64.to_le_bytes()); // TDUPDATE
    out.extend_from_slice(&(handseed.min(u64::from(u32::MAX)) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // the plot stamp
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    for value in [0u32, 0, 0, 1, 0, 0, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_cuts_at_the_section_size_and_ends_empty() {
        let entries: Vec<(u64, usize)> = (1..2000u64).map(|h| (h, h as usize * 40)).collect();
        let map = object_map(&entries);
        let (read, lost) =
            crate::sections::object_map(&map, usize::MAX, &crate::ImportLimits::default()).unwrap();
        assert_eq!(lost, 0);
        assert_eq!(read.len(), entries.len());
        assert_eq!((read[0].handle, read[0].offset), (1, 40));
        assert_eq!((read[1998].handle, read[1998].offset), (1999, 1999 * 40));
        assert_eq!(&map[map.len() - 4..map.len() - 2], &[0, 2]);
    }

    #[test]
    fn the_fixed_sections_have_the_specified_sizes() {
        assert_eq!(obj_free_space(3, 0x100).len(), 53);
        assert_eq!(aux_header(0x35, crate::Version::R2000).len(), 123);
        assert_eq!(preview().len(), 53);
    }
}
