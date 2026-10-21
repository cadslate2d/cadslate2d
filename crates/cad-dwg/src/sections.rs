//! A file opened: its version, its decoded sections, and its object map.

use std::borrow::Cow;

use crate::bits::BitReader;
use crate::{DwgError, ImportLimits, Version, r2000, r2004, r2007};

/// The sections a DWG file is made of, by the names the specification
/// gives them (`AcDb:…` in R2004 and later, locator numbers in R2000).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectionName {
    Header,
    Classes,
    /// The object map (`AcDb:Handles`).
    Handles,
    /// The objects (`AcDb:AcDbObjects`); in R2000 the objects are at
    /// absolute addresses of the file and this section is not listed.
    Objects,
    Template,
    Preview,
    SummaryInfo,
    AppInfo,
    FileDepList,
    RevHistory,
    ObjFreeSpace,
    AuxHeader,
    Security,
    VbaProject,
    /// The empty section of R2004 and later.
    Empty,
    /// R2000's locator 3, the "unknown section" of §25.
    Unknown,
    Other,
}

impl SectionName {
    pub(crate) fn parse(name: &str) -> Self {
        match name {
            "AcDb:Header" => Self::Header,
            "AcDb:Classes" => Self::Classes,
            "AcDb:Handles" => Self::Handles,
            "AcDb:AcDbObjects" => Self::Objects,
            "AcDb:Template" => Self::Template,
            "AcDb:Preview" => Self::Preview,
            "AcDb:SummaryInfo" => Self::SummaryInfo,
            "AcDb:AppInfo" => Self::AppInfo,
            "AcDb:FileDepList" => Self::FileDepList,
            "AcDb:RevHistory" => Self::RevHistory,
            "AcDb:ObjFreeSpace" => Self::ObjFreeSpace,
            "AcDb:AuxHeader" => Self::AuxHeader,
            "AcDb:Security" => Self::Security,
            "AcDb:VBAProject" => Self::VbaProject,
            "" => Self::Empty,
            _ => Self::Other,
        }
    }
}

/// A decoded section.
#[derive(Clone, Debug)]
pub struct Section {
    pub name: SectionName,
    pub data: Vec<u8>,
}

/// An entry of the object map (§23): a handle and where its object is in
/// the objects' bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub handle: u64,
    pub offset: usize,
}

/// A DWG file with its containers opened: the sections decoded, the
/// object map read. Objects are decoded on demand from `objects`.
#[derive(Debug)]
pub struct File<'a> {
    version: Version,
    limits: ImportLimits,
    sections: Vec<Section>,
    objects: Cow<'a, [u8]>,
    map: Vec<MapEntry>,
    unreachable: usize,
}

impl<'a> File<'a> {
    /// Opens a file with the default limits.
    pub fn open(bytes: &'a [u8]) -> Result<Self, DwgError> {
        Self::open_with_limits(bytes, &ImportLimits::default())
    }

    /// Opens a file: detects the version, decodes every section, checks
    /// what the container lets a reader check, and reads the object map.
    pub fn open_with_limits(bytes: &'a [u8], limits: &ImportLimits) -> Result<Self, DwgError> {
        if bytes.len() > limits.max_bytes {
            return Err(DwgError::FileTooLarge {
                bytes: bytes.len(),
                limit: limits.max_bytes,
            });
        }
        let version = Version::detect(bytes)?;
        let mut sections = match version {
            Version::R2000 => r2000::sections(bytes, limits)?,
            Version::R2004 => r2004::sections(bytes, limits)?,
            Version::R2007 => r2007::sections(bytes, limits)?,
        };
        let objects: Cow<'a, [u8]> = match version {
            Version::R2000 => Cow::Borrowed(bytes),
            _ => {
                let at = sections
                    .iter()
                    .position(|s| s.name == SectionName::Objects)
                    .ok_or(DwgError::MissingSection {
                        name: "AcDb:AcDbObjects",
                    })?;
                Cow::Owned(std::mem::take(&mut sections[at].data))
            }
        };
        let handles = sections
            .iter()
            .find(|s| s.name == SectionName::Handles)
            .ok_or(DwgError::MissingSection {
                name: "AcDb:Handles",
            })?;
        let (map, unreachable) = object_map(&handles.data, objects.len(), limits)?;
        Ok(Self {
            version,
            limits: *limits,
            sections,
            objects,
            map,
            unreachable,
        })
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn limits(&self) -> &ImportLimits {
        &self.limits
    }

    /// The sections found, in the file's order; the objects section's data
    /// is held apart and shows here as empty.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn section(&self, name: SectionName) -> Option<&[u8]> {
        self.sections
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.data.as_slice())
    }

    /// The bytes object offsets point into.
    pub fn object_bytes(&self) -> &[u8] {
        &self.objects
    }

    /// The object map: every object's handle and offset, in handle order.
    pub fn map(&self) -> &[MapEntry] {
        &self.map
    }

    /// Entries of the object map whose offset lies past the objects: a
    /// file may list objects it no longer holds (§20); they are left out
    /// of the map and counted here for the report.
    pub fn unreachable_objects(&self) -> usize {
        self.unreachable
    }
}

/// Reads the object map (§23.1): sections of up to 2032 bytes of modular
/// chars, each pair the offset of a handle from the last and of a location
/// from the last, ending with a section of size 2. Entries past the
/// objects are left out and counted.
pub(crate) fn object_map(
    d: &[u8],
    objects_len: usize,
    limits: &ImportLimits,
) -> Result<(Vec<MapEntry>, usize), DwgError> {
    let mut out = Vec::new();
    let mut unreachable = 0;
    let mut pos = 0usize;
    loop {
        // Each section's offsets start again from zero (ADR-0036 errata:
        // §23.1 reads as if they ran on; LibreDWG and the files reset).
        let mut handle: u64 = 0;
        let mut loc: i64 = 0;
        let size = d
            .get(pos..pos + 2)
            .map(|b| (usize::from(b[0]) << 8) | usize::from(b[1]))
            .ok_or(DwgError::Truncated {
                what: "the object map",
                at: pos,
            })?;
        if size == 2 {
            break;
        }
        if !(2..=2040).contains(&size) {
            return Err(DwgError::Corrupt {
                what: "an object map section size",
                at: pos,
            });
        }
        let end = pos + size;
        let body = d.get(pos + 2..end).ok_or(DwgError::Truncated {
            what: "the object map",
            at: pos,
        })?;
        let mut bits = BitReader::new(body);
        while bits.remaining_bits() >= 16 {
            if out.len() >= limits.max_objects {
                return Err(DwgError::TooMany {
                    what: "objects",
                    limit: limits.max_objects,
                });
            }
            handle = handle.wrapping_add(bits.umc()?);
            loc = loc.wrapping_add(bits.mc()?);
            match usize::try_from(loc).ok().filter(|&o| o < objects_len) {
                Some(offset) => out.push(MapEntry { handle, offset }),
                None => unreachable += 1,
            }
        }
        // The two CRC bytes close the section (§23.1).
        pos = end + 2;
    }
    Ok((out, unreachable))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_map_accumulates_handles_and_offsets() {
        // One section of size 9 (2 + 7 bytes): (1, 100), (+2, +50),
        // (+1, -20), the 100 in two bytes since a last byte holds six bits
        // and a sign; then its CRC; then the empty section.
        let d = [
            0x00, 0x09, 0x01, 0xE4, 0x00, 0x02, 0x32, 0x01, 0x54, 0xaa, 0xbb, 0x00, 0x02,
        ];
        let (map, unreachable) = object_map(&d, 1000, &ImportLimits::default()).unwrap();
        assert_eq!(unreachable, 0);
        assert_eq!(
            map,
            [
                MapEntry {
                    handle: 1,
                    offset: 100
                },
                MapEntry {
                    handle: 3,
                    offset: 150
                },
                MapEntry {
                    handle: 4,
                    offset: 130
                },
            ]
        );
        // An offset past the objects is left out and counted; a section
        // without its end and a count past the limit are errors.
        let (map, unreachable) = object_map(&d, 140, &ImportLimits::default()).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(unreachable, 1);
        assert!(matches!(
            object_map(&d[..6], 1000, &ImportLimits::default()),
            Err(DwgError::Truncated { .. })
        ));
        let small = ImportLimits {
            max_objects: 2,
            ..ImportLimits::default()
        };
        assert_eq!(
            object_map(&d, 1000, &small),
            Err(DwgError::TooMany {
                what: "objects",
                limit: 2
            })
        );
        assert!(matches!(
            object_map(&[0x09, 0x00], 10, &ImportLimits::default()),
            Err(DwgError::Corrupt { .. })
        ));
    }

    #[test]
    fn section_names_are_the_specifications() {
        assert_eq!(SectionName::parse("AcDb:AcDbObjects"), SectionName::Objects);
        assert_eq!(SectionName::parse(""), SectionName::Empty);
        assert_eq!(SectionName::parse("AcDb:Signature"), SectionName::Other);
    }
}
