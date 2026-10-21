//! The classes section written (ODA spec §10.1): the class of every
//! object type from 500 the file uses, between the sentinels.

use crate::Version;
use crate::classes::SENTINEL;
use crate::crc::crc16;
use crate::write::bits::BitWriter;

const SENTINEL_END: [u8; 16] = [
    0x72, 0x5E, 0x3B, 0x47, 0x3B, 0x56, 0x07, 0x3A, 0x3F, 0x23, 0x0B, 0xA0, 0x18, 0x30, 0x49, 0x75,
];

/// A class to define: the numbers are given in order from 500.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub dxf_name: String,
    pub cpp_name: String,
    pub app_name: String,
    pub proxy_flags: u16,
    pub was_zombie: bool,
    pub is_entity: bool,
}

/// The section: the sentinel, the size, each class, the CRC, the ending
/// sentinel. From R2004 the highest class number leads, and each class
/// carries how many objects it has and the version that wrote it
/// (§10.2).
pub fn section(classes: &[ClassDef], instances: &[u32], version: Version) -> Vec<u8> {
    let mut data = BitWriter::new();
    let r2004 = version >= Version::R2004;
    if r2004 {
        data.bs((499 + classes.len()) as i16);
        data.rc(0);
        data.rc(0);
        data.bit(1);
    }
    for (i, class) in classes.iter().enumerate() {
        data.bs((500 + i) as i16);
        data.bs(class.proxy_flags as i16);
        data.text(&class.app_name);
        data.text(&class.cpp_name);
        data.text(&class.dxf_name);
        data.bit(u8::from(class.was_zombie));
        data.bs(if class.is_entity { 0x1f2 } else { 0x1f3 });
        if r2004 {
            data.bl(instances.get(i).copied().unwrap_or(0) as i32);
            data.bs(i16::from(version.number()));
            data.bs(0); // the maintenance version
            data.bl(0);
            data.bl(0);
        }
    }
    data.align();
    let body = data.into_bytes();
    let mut out = Vec::with_capacity(body.len() + 38);
    out.extend_from_slice(&SENTINEL);
    let start = out.len();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    let crc = crc16(0xC0C1, &out[start..]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&SENTINEL_END);
    out
}
