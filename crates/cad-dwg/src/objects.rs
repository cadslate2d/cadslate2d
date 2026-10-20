//! Objects (ODA spec §20): each found through the object map, with its
//! type, handle, extended data, and its three streams — data, strings
//! (R2007), handles — and the common data every entity and every object
//! starts with.

use crate::bits::{BitReader, Color, Handle};
use crate::crc::crc16;
use crate::sections::{File, MapEntry};
use crate::{DwgError, ImportLimits, Version};

/// The fixed object types of §20.3 that are entities. Types of 500 and
/// above are classes, whose `item_class_id` says.
pub fn fixed_type_is_entity(type_code: u16) -> Option<bool> {
    Some(match type_code {
        0x01..=0x29 | 0x2b..=0x2f | 0x4a | 0x4d | 0x4e | 0x1f2 => true,
        0x2a | 0x30..=0x49 | 0x4b | 0x4c | 0x4f..=0x52 | 0x1f3 => false,
        0 | 0x53..=0x1f1 | 0x1f4.. => return None,
    })
}

/// Fixed type codes this crate names (§20.3).
pub mod types {
    pub const TEXT: u16 = 0x01;
    pub const ATTRIB: u16 = 0x02;
    pub const ATTDEF: u16 = 0x03;
    pub const BLOCK: u16 = 0x04;
    pub const ENDBLK: u16 = 0x05;
    pub const SEQEND: u16 = 0x06;
    pub const INSERT: u16 = 0x07;
    pub const MINSERT: u16 = 0x08;
    pub const VERTEX_2D: u16 = 0x0a;
    pub const VERTEX_3D: u16 = 0x0b;
    pub const POLYLINE_2D: u16 = 0x0f;
    pub const POLYLINE_3D: u16 = 0x10;
    pub const ARC: u16 = 0x11;
    pub const CIRCLE: u16 = 0x12;
    pub const LINE: u16 = 0x13;
    pub const DIMENSION_ORDINATE: u16 = 0x14;
    pub const DIMENSION_LINEAR: u16 = 0x15;
    pub const DIMENSION_ALIGNED: u16 = 0x16;
    pub const DIMENSION_ANGULAR_3PT: u16 = 0x17;
    pub const DIMENSION_ANGULAR_2LN: u16 = 0x18;
    pub const DIMENSION_RADIUS: u16 = 0x19;
    pub const DIMENSION_DIAMETER: u16 = 0x1a;
    pub const POINT: u16 = 0x1b;
    pub const FACE_3D: u16 = 0x1c;
    pub const SOLID: u16 = 0x1f;
    pub const VIEWPORT: u16 = 0x22;
    pub const ELLIPSE: u16 = 0x23;
    pub const SPLINE: u16 = 0x24;
    pub const RAY: u16 = 0x28;
    pub const XLINE: u16 = 0x29;
    pub const DICTIONARY: u16 = 0x2a;
    pub const MTEXT: u16 = 0x2c;
    pub const LEADER: u16 = 0x2d;
    pub const TOLERANCE: u16 = 0x2e;
    pub const MLINE: u16 = 0x2f;
    pub const BLOCK_CONTROL: u16 = 0x30;
    pub const BLOCK_HEADER: u16 = 0x31;
    pub const LAYER_CONTROL: u16 = 0x32;
    pub const LAYER: u16 = 0x33;
    pub const STYLE_CONTROL: u16 = 0x34;
    pub const STYLE: u16 = 0x35;
    pub const LTYPE_CONTROL: u16 = 0x38;
    pub const LTYPE: u16 = 0x39;
    pub const VIEW_CONTROL: u16 = 0x3c;
    pub const VIEW: u16 = 0x3d;
    pub const UCS_CONTROL: u16 = 0x3e;
    pub const UCS: u16 = 0x3f;
    pub const VPORT_CONTROL: u16 = 0x40;
    pub const VPORT: u16 = 0x41;
    pub const APPID_CONTROL: u16 = 0x42;
    pub const APPID: u16 = 0x43;
    pub const DIMSTYLE_CONTROL: u16 = 0x44;
    pub const DIMSTYLE: u16 = 0x45;
    pub const VIEWPORT_ENTITY_HEADER_CONTROL: u16 = 0x46;
    pub const VIEWPORT_ENTITY_HEADER: u16 = 0x47;
    pub const GROUP: u16 = 0x48;
    pub const MLINESTYLE: u16 = 0x49;
    pub const LWPOLYLINE: u16 = 0x4d;
    pub const HATCH: u16 = 0x4e;
    pub const XRECORD: u16 = 0x4f;
    pub const PLACEHOLDER: u16 = 0x50;
    pub const LAYOUT: u16 = 0x52;
    pub const PROXY_ENTITY: u16 = 0x1f2;
    pub const PROXY_OBJECT: u16 = 0x1f3;
    /// The first class-defined type (§20.3).
    pub const FIRST_CLASS: u16 = 500;
}

/// An object as the map found it: its bytes, not yet decoded past the
/// type and the size of its data in bits.
#[derive(Clone, Copy, Debug)]
pub struct RawObject<'a> {
    /// The handle the object map lists it under.
    pub handle: u64,
    /// Where it starts in the objects' bytes.
    pub offset: usize,
    pub type_code: u16,
    /// The object's bytes: from its type to the end of its handle stream,
    /// without the size before and the CRC after.
    pub data: &'a [u8],
    /// Bits of data before the handle stream (§20.1).
    pub bitsize: usize,
    /// Whether the CRC after the object matches (§20.1).
    pub crc_matches: bool,
}

/// The three streams of an object (§20.1), positioned after its header:
/// the data after the extended data, the strings at their start (R2007),
/// the handles at theirs.
#[derive(Debug)]
pub struct Streams<'a> {
    pub data: BitReader<'a>,
    pub strings: Option<BitReader<'a>>,
    pub handles: BitReader<'a>,
}

impl<'a> Streams<'a> {
    /// A text field: from the string stream where there is one, else
    /// from the data stream, as bytes in the drawing's code page or as
    /// Unicode decoded to UTF-8.
    pub fn text(&mut self, limits: &ImportLimits) -> Result<String, DwgError> {
        match self.strings.as_mut() {
            Some(strings) => strings.text_unicode(limits),
            None => Ok(String::from_utf8_lossy(&self.data.text(limits)?).into_owned()),
        }
    }
}

/// What every object starts with (§20.1): the handle, and the extended
/// entity data, read and kept as bytes per application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectHeader {
    pub handle: Handle,
    /// Extended entity data (§28): the application handle and its bytes.
    pub eed: Vec<(Handle, Vec<u8>)>,
}

/// The common data of an entity (§20.4.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityCommon {
    pub has_preview: bool,
    /// 0: owned by a block or a polyline, 1: paper space, 2: model space.
    pub entmode: u8,
    pub num_reactors: u32,
    pub xdic_missing: bool,
    pub nolinks: bool,
    pub color: Color,
    pub ltype_scale: f64,
    pub ltype_flags: u8,
    pub plotstyle_flags: u8,
    pub material_flags: u8,
    pub shadow_flags: u8,
    pub invisible: bool,
    pub lineweight: u8,
}

/// The common data of a non-entity object (§20.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectCommon {
    pub num_reactors: u32,
    pub xdic_missing: bool,
}

/// The common handles of an entity (§20.4.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityHandles {
    pub owner: Option<Handle>,
    pub reactors: Vec<Handle>,
    pub xdic: Option<Handle>,
    pub prev: Option<Handle>,
    pub next: Option<Handle>,
    pub color_book: Option<Handle>,
    pub layer: Handle,
    pub linetype: Option<Handle>,
    pub material: Option<Handle>,
    pub plotstyle: Option<Handle>,
}

/// The common handles of a non-entity object (§20.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectHandles {
    pub owner: Handle,
    pub reactors: Vec<Handle>,
    pub xdic: Option<Handle>,
}

impl<'a> File<'a> {
    /// The object a map entry points at: its size read, its CRC checked,
    /// its type and bit size read.
    pub fn object(&self, entry: MapEntry) -> Result<RawObject<'_>, DwgError> {
        let bytes = self.object_bytes();
        let mut head = BitReader::new(bytes.get(entry.offset..).ok_or(DwgError::Corrupt {
            what: "an object map offset",
            at: entry.offset,
        })?);
        let size = head.ms().map_err(|_| DwgError::Truncated {
            what: "an object's size",
            at: entry.offset,
        })? as usize;
        if size > self.limits().max_object_bytes {
            return Err(DwgError::TooMany {
                what: "bytes in an object",
                limit: self.limits().max_object_bytes,
            });
        }
        let start = entry.offset + head.byte_position();
        let data = bytes.get(start..start + size).ok_or(DwgError::Truncated {
            what: "an object",
            at: start,
        })?;
        let stored_crc = bytes
            .get(start + size..start + size + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]));
        let crc_matches = stored_crc == Some(crc16(0xC0C1, &bytes[entry.offset..start + size]));
        let mut bits = BitReader::new(data);
        let type_code = bits.bs()? as u16;
        let bitsize = bits.rl()? as usize;
        if bitsize > size * 8 {
            return Err(DwgError::Corrupt {
                what: "an object's bit size",
                at: start,
            });
        }
        Ok(RawObject {
            handle: entry.handle,
            offset: entry.offset,
            type_code,
            data,
            bitsize,
            crc_matches,
        })
    }

    /// Every object of the map, in handle order.
    pub fn objects(&self) -> impl Iterator<Item = Result<RawObject<'_>, DwgError>> + '_ {
        self.map().iter().map(|entry| self.object(*entry))
    }

    /// Opens an object's streams and reads its header: the handle and the
    /// extended data.
    pub fn open_object(
        &self,
        object: &RawObject<'a>,
    ) -> Result<(ObjectHeader, Streams<'a>), DwgError> {
        let limits = self.limits();
        let full = BitReader::new(object.data);
        let strings = if self.version() >= Version::R2007 {
            string_stream(&full, object.bitsize)?
        } else {
            None
        };
        let handles = full.window(object.bitsize, object.data.len() * 8)?;
        let mut data = full;
        data.bs()?;
        data.rl()?;
        let handle = data.handle()?;
        let mut eed = Vec::new();
        loop {
            let n = data.bs()?;
            if n == 0 {
                break;
            }
            let n = usize::try_from(n).map_err(|_| DwgError::Corrupt {
                what: "extended data",
                at: object.offset,
            })?;
            if n > limits.max_string_bytes || n * 8 > data.remaining_bits() {
                return Err(DwgError::Corrupt {
                    what: "extended data",
                    at: object.offset,
                });
            }
            let app = data.handle()?;
            let mut bytes = Vec::with_capacity(n);
            for _ in 0..n {
                bytes.push(data.rc()?);
            }
            eed.push((app, bytes));
        }
        Ok((
            ObjectHeader { handle, eed },
            Streams {
                data,
                strings,
                handles,
            },
        ))
    }

    /// Reads an entity's common data (§20.4.1) from a data stream
    /// positioned after the extended data.
    pub fn entity_common(
        &self,
        streams: &mut Streams<'_>,
        object: &RawObject<'_>,
    ) -> Result<EntityCommon, DwgError> {
        let version = self.version();
        let data = &mut streams.data;
        let has_preview = data.bit()? == 1;
        let mut common = EntityCommon {
            has_preview,
            ..EntityCommon::default()
        };
        if common.has_preview {
            let n = data.rl()? as usize;
            if n * 8 > data.remaining_bits() {
                return Err(DwgError::Corrupt {
                    what: "a preview",
                    at: object.offset,
                });
            }
            data.set_position(data.position() + n * 8)?;
        }
        common.entmode = data.bb()?;
        common.num_reactors = u32::try_from(data.bl()?).map_err(|_| DwgError::Corrupt {
            what: "a reactor count",
            at: object.offset,
        })?;
        if version >= Version::R2004 {
            common.xdic_missing = data.bit()? == 1;
        }
        // ADR-0036 errata 1: the specification says the bit is "always 1"
        // in R2004 and later; it is not stored.
        common.nolinks = if version < Version::R2004 {
            data.bit()? == 1
        } else {
            true
        };
        common.color = data.enc(version)?;
        common.ltype_scale = data.bd()?;
        common.ltype_flags = data.bb()?;
        common.plotstyle_flags = data.bb()?;
        if version >= Version::R2007 {
            common.material_flags = data.bb()?;
            common.shadow_flags = data.rc()?;
        }
        common.invisible = data.bs()? & 1 != 0;
        common.lineweight = data.rc()?;
        Ok(common)
    }

    /// Reads a non-entity object's common data (§20.1).
    pub fn object_common(
        &self,
        streams: &mut Streams<'_>,
        object: &RawObject<'_>,
    ) -> Result<ObjectCommon, DwgError> {
        let data = &mut streams.data;
        let num_reactors = u32::try_from(data.bl()?).map_err(|_| DwgError::Corrupt {
            what: "a reactor count",
            at: object.offset,
        })?;
        let xdic_missing = self.version() >= Version::R2004 && data.bit()? == 1;
        Ok(ObjectCommon {
            num_reactors,
            xdic_missing,
        })
    }

    /// Reads an entity's common handles (§20.4.2) from the handle stream.
    pub fn entity_handles(
        &self,
        streams: &mut Streams<'_>,
        common: &EntityCommon,
    ) -> Result<EntityHandles, DwgError> {
        let version = self.version();
        let h = &mut streams.handles;
        let mut out = EntityHandles::default();
        if common.entmode == 0 {
            out.owner = Some(h.handle()?);
        }
        for _ in 0..common.num_reactors {
            out.reactors.push(h.handle()?);
        }
        if !(version >= Version::R2004 && common.xdic_missing) {
            out.xdic = Some(h.handle()?);
        }
        if version < Version::R2004 && !common.nolinks {
            out.prev = Some(h.handle()?);
            out.next = Some(h.handle()?);
        }
        if version >= Version::R2004 && common.color.has_book_handle {
            out.color_book = Some(h.handle()?);
        }
        out.layer = h.handle()?;
        if common.ltype_flags == 3 {
            out.linetype = Some(h.handle()?);
        }
        if version >= Version::R2007 && common.material_flags == 3 {
            out.material = Some(h.handle()?);
        }
        if common.plotstyle_flags == 3 {
            out.plotstyle = Some(h.handle()?);
        }
        Ok(out)
    }

    /// Reads a non-entity object's common handles (§20.1).
    pub fn object_handles(
        &self,
        streams: &mut Streams<'_>,
        common: &ObjectCommon,
    ) -> Result<ObjectHandles, DwgError> {
        let h = &mut streams.handles;
        let owner = h.handle()?;
        let mut reactors = Vec::new();
        for _ in 0..common.num_reactors {
            reactors.push(h.handle()?);
        }
        let xdic = if self.version() >= Version::R2004 && common.xdic_missing {
            None
        } else {
            Some(h.handle()?)
        };
        Ok(ObjectHandles {
            owner,
            reactors,
            xdic,
        })
    }
}

/// The string stream of an R2007 object or section (§20.1): the last bit
/// before the handles says whether there is one; before it, a short with
/// the stream's size in bits, with a second short above when the high bit
/// is set; the stream ends where the size begins.
pub(crate) fn string_stream<'a>(
    full: &BitReader<'a>,
    endbit: usize,
) -> Result<Option<BitReader<'a>>, DwgError> {
    if endbit < 1 {
        return Ok(None);
    }
    let mut probe = full.window(endbit - 1, full.end())?;
    if probe.bit()? == 0 {
        return Ok(None);
    }
    let mut start = endbit.checked_sub(17).ok_or(DwgError::Corrupt {
        what: "a string stream",
        at: 0,
    })?;
    let mut probe = full.window(start, full.end())?;
    let mut size = usize::from(probe.rs()?);
    if size & 0x8000 != 0 {
        start = start.checked_sub(16).ok_or(DwgError::Corrupt {
            what: "a string stream",
            at: 0,
        })?;
        let mut probe = full.window(start, full.end())?;
        let hi = usize::from(probe.rs()?);
        size = (size & 0x7fff) | (hi << 15);
    }
    let stream_start = start.checked_sub(size).ok_or(DwgError::Corrupt {
        what: "a string stream",
        at: 0,
    })?;
    Ok(Some(full.window(stream_start, start)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_types_are_sorted_into_entities_and_objects() {
        assert_eq!(fixed_type_is_entity(types::LINE), Some(true));
        assert_eq!(fixed_type_is_entity(types::LWPOLYLINE), Some(true));
        assert_eq!(fixed_type_is_entity(types::HATCH), Some(true));
        assert_eq!(fixed_type_is_entity(types::MTEXT), Some(true));
        assert_eq!(fixed_type_is_entity(types::PROXY_ENTITY), Some(true));
        assert_eq!(fixed_type_is_entity(types::DICTIONARY), Some(false));
        assert_eq!(fixed_type_is_entity(types::LAYER), Some(false));
        assert_eq!(fixed_type_is_entity(types::LAYOUT), Some(false));
        assert_eq!(fixed_type_is_entity(types::XRECORD), Some(false));
        assert_eq!(fixed_type_is_entity(types::FIRST_CLASS), None);
        assert_eq!(fixed_type_is_entity(0), None);
    }

    #[test]
    fn a_string_stream_is_found_from_the_bit_before_the_handles() {
        // 8 bytes of strings, then the size short (64, little-endian),
        // then the present bit, then handles.
        let mut data = vec![0xabu8; 8];
        data.extend(64u16.to_le_bytes());
        data.push(0x80);
        let full = BitReader::new(&data);
        let endbit = 8 * 8 + 16 + 1;
        let s = string_stream(&full, endbit).unwrap().unwrap();
        assert_eq!(s.position(), 0);
        assert_eq!(s.end(), 64);
        // Absent.
        data[10] = 0;
        let full = BitReader::new(&data);
        assert!(string_stream(&full, endbit).unwrap().is_none());
        // A size past the start is corrupt.
        let mut data = vec![0u8; 2];
        data.extend(64u16.to_le_bytes());
        data.push(0x80);
        let full = BitReader::new(&data);
        assert!(matches!(
            string_stream(&full, 2 * 8 + 17),
            Err(DwgError::Corrupt { .. })
        ));
    }
}
