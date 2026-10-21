//! The classes section written (ODA spec §10.1): the class of every
//! object type from 500 the file uses, between the sentinels.

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
/// sentinel.
pub fn section(classes: &[ClassDef]) -> Vec<u8> {
    let mut data = BitWriter::new();
    for (i, class) in classes.iter().enumerate() {
        data.bs((500 + i) as i16);
        data.bs(class.proxy_flags as i16);
        data.text(&class.app_name);
        data.text(&class.cpp_name);
        data.text(&class.dxf_name);
        data.bit(u8::from(class.was_zombie));
        data.bs(if class.is_entity { 0x1f2 } else { 0x1f3 });
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
