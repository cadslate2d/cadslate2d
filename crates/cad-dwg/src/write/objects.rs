//! One object's envelope (ODA spec §20.1, §20.4.1, §20.4.2): the type, the
//! size in bits of the data before the handle stream, the handle, no
//! extended data, the common data of an entity or an object, then the
//! object's own fields and its handles; the size before and the CRC after.

use crate::Version;
use crate::crc::crc16;
use crate::write::bits::BitWriter;

/// An object being written: its data stream and its handle stream, joined
/// when it is finished.
#[derive(Debug)]
pub struct ObjectWriter {
    pub type_code: u16,
    pub handle: u64,
    pub version: Version,
    pub data: BitWriter,
    /// R2007: the texts, in a stream of their own after the data (§20.1).
    pub strings: BitWriter,
    pub handles: BitWriter,
}

/// An object written: its handle, for the map, and its bytes — the
/// modular-short size, the data, the CRC (§20.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectBytes {
    pub handle: u64,
    pub bytes: Vec<u8>,
}

/// The common data of an entity as this crate writes it (§20.4.1): no
/// preview, no reactors, links stored (R2000), no extension dictionary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityCommon {
    /// 0: owned by a block or an insert, 1: paper space, 2: model space.
    pub entmode: u8,
    /// The colour index: 256 by layer, 0 by block.
    pub color: i16,
    pub ltype_scale: f64,
    /// 0 by layer, 1 by block, 2 continuous, 3 a linetype by handle.
    pub ltype_flags: u8,
    pub invisible: bool,
    /// The lineweight index of §20.4.1: 29 by layer, 30 by block, 31 the
    /// default.
    pub lineweight: u8,
}

/// The handles every entity names (§20.4.2), in the order they are
/// stored, each `None` written as a null reference of its code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityLinks {
    /// The owner, stored only when the entity is owned (entmode 0).
    pub owner: u64,
    pub prev: u64,
    pub next: u64,
    pub layer: u64,
    /// The linetype, stored only when the flags say so (3).
    pub linetype: u64,
}

impl ObjectWriter {
    /// Begins an object: its handle and no extended data in the data
    /// stream; the type and the size go in front when it is finished.
    pub fn new(type_code: u16, handle: u64, version: Version) -> Self {
        let mut data = BitWriter::new();
        data.handle(0, handle);
        data.bs(0);
        Self {
            type_code,
            handle,
            version,
            data,
            strings: BitWriter::new(),
            handles: BitWriter::new(),
        }
    }

    /// Whether the file is R2004 or later: no links, the extension
    /// dictionary's absence flagged, owned objects listed.
    pub fn r2004(&self) -> bool {
        self.version >= Version::R2004
    }

    /// Whether the file is R2007: texts in the string stream, as Unicode.
    pub fn r2007(&self) -> bool {
        self.version >= Version::R2007
    }

    /// A text field: into the string stream as UTF-16 in R2007, else
    /// into the data as bytes (§20.1).
    pub fn text(&mut self, value: &str) {
        if self.r2007() {
            self.strings.text_unicode(value);
        } else {
            self.data.text(value);
        }
    }

    /// The common data of an entity (§20.4.1), after the extended data.
    pub fn entity_common(&mut self, common: &EntityCommon) {
        let r2004 = self.r2004();
        let d = &mut self.data;
        d.bit(0); // no preview
        d.bb(common.entmode);
        d.bl(0); // reactors
        if r2004 {
            d.bit(1); // no extension dictionary
        } else {
            d.bit(0); // links stored (nolinks = 0)
        }
        d.enc(common.color);
        d.bd(common.ltype_scale);
        d.bb(common.ltype_flags);
        d.bb(0); // plot style by layer
        if self.version >= Version::R2007 {
            d.bb(0); // material by layer
            d.rc(0); // shadows as the layer's
        }
        d.bs(i16::from(common.invisible));
        d.rc(common.lineweight);
    }

    /// The common handles of an entity (§20.4.2): the owner when owned,
    /// no reactors, a null extension dictionary and the links (R2000),
    /// the layer, the linetype when the flags name one.
    pub fn entity_handles(&mut self, common: &EntityCommon, links: &EntityLinks) {
        let r2004 = self.r2004();
        let h = &mut self.handles;
        if common.entmode == 0 {
            h.handle(4, links.owner);
        }
        if !r2004 {
            h.handle(3, 0); // xdictionary
            h.handle(4, links.prev);
            h.handle(4, links.next);
        }
        h.handle(5, links.layer);
        if common.ltype_flags == 3 {
            h.handle(5, links.linetype);
        }
    }

    /// The common data of a non-entity object (§20.1): no reactors; from
    /// R2004, whether an extension dictionary follows in the handles.
    pub fn object_common(&mut self, xdic: u64) {
        self.data.bl(0);
        if self.r2004() {
            self.data.bit(u8::from(xdic == 0));
        }
    }

    /// The common handles of a non-entity object (§20.1): the owner, no
    /// reactors, the extension dictionary — null in R2000, left out from
    /// R2004 when there is none.
    pub fn object_handles(&mut self, owner: u64, xdic: u64) {
        self.handles.handle(4, owner);
        if xdic != 0 || !self.r2004() {
            self.handles.handle(3, xdic);
        }
    }

    /// CMC (§2.11): the index in R2000; from R2004 a zero index, then the
    /// long with its method byte — by layer, by block, or an index — and
    /// no names.
    pub fn cmc(&mut self, index: i16) {
        if !self.r2004() {
            self.data.cmc(index);
            return;
        }
        self.data.bs(0);
        let value: u32 = match index {
            256 => 0xC000_0000,
            0 => 0xC100_0000,
            i => 0xC300_0000 | u32::from(i as u16 & 0xff),
        };
        self.data.bl(value as i32);
        self.data.rc(0);
    }

    /// The object's bytes: the type, the bit size of the data, the data —
    /// in R2007 with the string stream, its size, and the bit that says it
    /// is there (§20.1) — the handles, padded to a byte; the size in
    /// front, the CRC after.
    pub fn finish(self) -> ObjectBytes {
        let body = with_strings(self.data, &self.strings, self.version);
        let mut head = BitWriter::new();
        head.bs(self.type_code as i16);
        let bitsize = head.position() + 32 + body.position();
        head.rl(bitsize as u32);
        head.append(&body);
        head.append(&self.handles);
        head.align();
        let body = head.into_bytes();
        let mut out = BitWriter::new();
        out.ms(body.len() as u32);
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(&body);
        let crc = crc16(0xC0C1, &bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        ObjectBytes {
            handle: self.handle,
            bytes,
        }
    }
}

/// The data stream closed for its version: in R2007 the string stream
/// follows the data, then its size in bits — one short, or two with the
/// high bit of the second set when it is 0x8000 or more — and the bit
/// that says it is there; a zero bit when there are no strings (§20.1).
/// The header and the classes sections frame their streams the same
/// way (§5.8, §9).
pub fn with_strings(data: BitWriter, strings: &BitWriter, version: Version) -> BitWriter {
    let mut body = data;
    if version >= Version::R2007 {
        let size = strings.position();
        if size == 0 {
            body.bit(0);
        } else {
            body.append(strings);
            if size >= 0x8000 {
                body.rs((size >> 15) as u16);
                body.rs((size & 0x7fff) as u16 | 0x8000);
            } else {
                body.rs(size as u16);
            }
            body.bit(1);
        }
    }
    body
}

/// A section of streams (§5.8, §9): in R2007 the bit size of what comes
/// before the handles — counted from the size itself — then the data
/// with its strings, then the handles; before R2007 the data alone.
pub fn section_streams(
    data: BitWriter,
    strings: &BitWriter,
    handles: &BitWriter,
    version: Version,
) -> Vec<u8> {
    if version < Version::R2007 {
        let mut data = data;
        data.align();
        return data.into_bytes();
    }
    let body = with_strings(data, strings, version);
    let mut head = BitWriter::new();
    head.rl((32 + body.position()) as u32);
    head.append(&body);
    head.append(handles);
    head.align();
    head.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is written is what `File::object` and `open_object` read: the
    /// size, the CRC, the bit size, the handle, and the streams.
    #[test]
    fn an_object_is_framed_as_the_reader_expects() {
        let mut w = ObjectWriter::new(0x13, 0x2A, Version::R2000);
        w.entity_common(&EntityCommon {
            entmode: 2,
            color: 256,
            ltype_scale: 1.0,
            ltype_flags: 0,
            invisible: false,
            lineweight: 29,
        });
        w.data.bd(7.5);
        w.entity_handles(
            &EntityCommon {
                entmode: 2,
                color: 256,
                ltype_scale: 1.0,
                ltype_flags: 0,
                invisible: false,
                lineweight: 29,
            },
            &EntityLinks {
                layer: 0x11,
                ..EntityLinks::default()
            },
        );
        let object = w.finish();
        let mut r = crate::bits::BitReader::new(&object.bytes);
        let size = r.ms().unwrap() as usize;
        let start = r.byte_position();
        assert_eq!(object.bytes.len(), start + size + 2);
        let crc = u16::from_le_bytes([object.bytes[start + size], object.bytes[start + size + 1]]);
        assert_eq!(crc, crc16(0xC0C1, &object.bytes[..start + size]));
        let mut d = crate::bits::BitReader::new(&object.bytes[start..start + size]);
        assert_eq!(d.bs().unwrap(), 0x13);
        let bitsize = d.rl().unwrap() as usize;
        let handle = d.handle().unwrap();
        assert_eq!((handle.code, handle.value), (0, 0x2A));
        assert_eq!(d.bs().unwrap(), 0); // no EED
        assert_eq!(d.bit().unwrap(), 0); // no preview
        assert_eq!(d.bb().unwrap(), 2);
        let mut h = d.window(bitsize, size * 8).unwrap();
        let xdic = h.handle().unwrap();
        assert_eq!((xdic.code, xdic.value), (3, 0));
        h.handle().unwrap();
        h.handle().unwrap();
        let layer = h.handle().unwrap();
        assert_eq!((layer.code, layer.value), (5, 0x11));
        assert!(h.remaining_bits() < 8);
    }
}
