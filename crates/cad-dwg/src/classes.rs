//! The classes section (ODA spec §10): what the object types of 500 and
//! above mean in this drawing.

use crate::bits::BitReader;
use crate::objects::string_stream;
use crate::sections::{File, SectionName};
use crate::{DwgError, Version};

pub(crate) const SENTINEL: [u8; 16] = [
    0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A,
];

/// A class (§10): its number is the object type of its instances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    pub number: u16,
    pub proxy_flags: u16,
    pub app_name: String,
    pub cpp_name: String,
    pub dxf_name: String,
    pub was_zombie: bool,
    /// 0x1F2 for a class whose instances are entities, 0x1F3 for objects.
    pub item_class_id: u16,
}

impl Class {
    pub fn is_entity(&self) -> bool {
        self.item_class_id == 0x1f2
    }
}

impl File<'_> {
    /// The classes the drawing defines.
    pub fn classes(&self) -> Result<Vec<Class>, DwgError> {
        let section = self
            .section(SectionName::Classes)
            .ok_or(DwgError::MissingSection {
                name: "AcDb:Classes",
            })?;
        let at = section
            .windows(16)
            .take(64)
            .position(|w| w == SENTINEL)
            .ok_or(DwgError::Corrupt {
                what: "the classes sentinel",
                at: 0,
            })?;
        let section = &section[at..];
        let version = self.version();
        let limits = self.limits();
        let full = BitReader::new(section);
        let mut data = full.window(16 * 8, full.end())?;
        let size = data.rl()? as usize;
        let mut strings = None;
        if version >= Version::R2007 {
            let bitsize = data.rl()? as usize;
            // The streams are placed like an object's, from after the
            // sentinel and the size (§5.8).
            strings = string_stream(&full, bitsize + 160)?;
        }
        let end = (20 + size) * 8;
        if end > full.end() {
            return Err(DwgError::Truncated {
                what: "the classes section",
                at: 20,
            });
        }
        let mut data = full.window(data.position(), end)?;
        // R2000 reads classes until the data is exhausted; R2004 and later
        // say how many there are through the highest class number.
        let mut count = usize::MAX;
        if version >= Version::R2004 {
            let max_number = data.bs()?;
            data.rc()?;
            data.rc()?;
            data.bit()?;
            count = usize::try_from(max_number)
                .ok()
                .and_then(|n| n.checked_sub(499))
                .ok_or(DwgError::Corrupt {
                    what: "the highest class number",
                    at: 20,
                })?;
        }
        let mut classes = Vec::new();
        while classes.len() < count && data.remaining_bits() >= 13 {
            if classes.len() >= limits.max_classes {
                return Err(DwgError::TooMany {
                    what: "classes",
                    limit: limits.max_classes,
                });
            }
            let number = data.bs()? as u16;
            let proxy_flags = data.bs()? as u16;
            let mut text = || -> Result<String, DwgError> {
                match strings.as_mut() {
                    Some(s) => s.text_unicode(limits),
                    None => Ok(String::from_utf8_lossy(&data.text(limits)?).into_owned()),
                }
            };
            let app_name = text()?;
            let cpp_name = text()?;
            let dxf_name = text()?;
            let was_zombie = data.bit()? == 1;
            let item_class_id = data.bs()? as u16;
            if version >= Version::R2004 {
                data.bl()?; // instances
                data.bs()?; // the DWG version
                data.bs()?; // the maintenance version
                data.bl()?;
                data.bl()?;
            }
            classes.push(Class {
                number,
                proxy_flags,
                app_name,
                cpp_name,
                dxf_name,
                was_zombie,
                item_class_id,
            });
        }
        Ok(classes)
    }
}
