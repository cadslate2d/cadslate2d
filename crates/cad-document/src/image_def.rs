//! Image definitions (ADR-0042 §1): the pictures a drawing's images show.
//!
//! A definition is a row of its own table, as DXF's `IMAGEDEF` is: a name,
//! the encoded bytes the host made (§3), and what they are — the format,
//! the pixel size, and a fingerprint of the bytes that names them in files
//! and finds the same picture attached twice. The engine never decodes a
//! picture: it reads the size from the header, to check what it is told,
//! and hands the bytes to whoever draws them.
//!
//! Only what the host makes is a definition: a PNG of opaque 8-bit grey or
//! colour, not interlaced, or a JPEG of one or three components. So a plot
//! embeds either without decoding it (ADR-0042 §5).

use std::sync::Arc;

use crate::names::{NameError, Named, check_name};
use crate::table::{Table, TableChange, TableId};

/// The largest picture a definition holds, each way, in pixels: four times
/// the 2,048 the host makes, room for a picture from another program.
pub const MOST_PIXELS: u32 = 8192;

/// The most bytes a definition holds: a 2,048-pixel photograph is under two
/// megabytes.
pub const MOST_BYTES: usize = 16 << 20;

/// How a definition's bytes are encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// Its media type.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// Its usual file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// Why bytes are not a definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageDefError {
    /// The name is empty or holds a character a name cannot.
    BadName(NameError),
    /// Neither a PNG nor a JPEG, or one cut short.
    NotAnImage,
    /// A PNG or JPEG of a kind the host does not make: what it is.
    Unsupported(&'static str),
    /// Wider or taller than [`MOST_PIXELS`], or of no size.
    BadSize { width: u32, height: u32 },
    /// More bytes than [`MOST_BYTES`].
    TooLarge(usize),
}

impl ImageDefError {
    /// The error as a sentence, for a command's log.
    pub fn explain(&self) -> String {
        match self {
            Self::BadName(error) => format!("The image's name is not allowed: {error:?}."),
            Self::NotAnImage => "That is not a PNG or JPEG image.".to_owned(),
            Self::Unsupported(what) => format!("That image is {what}, which is not taken."),
            Self::BadSize { width, height } => format!(
                "That image is {width} × {height} pixels; an image is at most {MOST_PIXELS} each way."
            ),
            Self::TooLarge(bytes) => format!(
                "That image is {} MB; an image is at most {} MB.",
                bytes >> 20,
                MOST_BYTES >> 20
            ),
        }
    }
}

/// A picture a drawing's images show.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageDef {
    pub name: String,
    pub format: ImageFormat,
    /// Its size in pixels, as its header says.
    pub width: u32,
    pub height: u32,
    /// The bytes, encoded as the host made them; shared, never copied.
    pub bytes: Arc<[u8]>,
    /// [`fingerprint`] of the bytes.
    pub digest: u64,
}

impl ImageDef {
    /// The definition `bytes` make, called `name`: its format and size read
    /// from its header, refused unless it is a picture the host makes.
    pub fn new(
        name: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, ImageDefError> {
        let name = name.into();
        check_name(&name).map_err(ImageDefError::BadName)?;
        let bytes: Arc<[u8]> = bytes.into();
        if bytes.len() > MOST_BYTES {
            return Err(ImageDefError::TooLarge(bytes.len()));
        }
        let (format, width, height) = header(&bytes)?;
        if width == 0 || height == 0 || width > MOST_PIXELS || height > MOST_PIXELS {
            return Err(ImageDefError::BadSize { width, height });
        }
        Ok(Self {
            name,
            format,
            width,
            height,
            digest: fingerprint(&bytes),
            bytes,
        })
    }

    /// A definition whose picture is not in the drawing (ADR-0042 §4): a
    /// file another program named, which the browser cannot open by path.
    /// It keeps its name and size, and has no bytes until it is reloaded;
    /// its images show their frames and no picture.
    pub fn missing(
        name: impl Into<String>,
        width: u32,
        height: u32,
    ) -> Result<Self, ImageDefError> {
        let name = name.into();
        check_name(&name).map_err(ImageDefError::BadName)?;
        if width == 0 || height == 0 || width > MOST_PIXELS || height > MOST_PIXELS {
            return Err(ImageDefError::BadSize { width, height });
        }
        Ok(Self {
            name,
            format: ImageFormat::Png,
            width,
            height,
            bytes: Arc::from([]),
            digest: 0,
        })
    }

    /// Whether its picture is not in the drawing.
    pub fn is_missing(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Whether it is what its bytes say: the definition [`Self::new`] makes
    /// of them, with its name — or, missing its picture, what
    /// [`Self::missing`] makes. A row built otherwise is refused.
    pub fn check(&self) -> Result<(), ImageDefError> {
        let made = if self.is_missing() {
            Self::missing(self.name.clone(), self.width, self.height)?
        } else {
            Self::new(self.name.clone(), Arc::clone(&self.bytes))?
        };
        if made != *self {
            return Err(ImageDefError::Unsupported(
                "described otherwise than its bytes",
            ));
        }
        Ok(())
    }
}

impl Named for ImageDef {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type ImageDefId = TableId<ImageDef>;
pub type ImageDefTable = Table<ImageDef>;
/// A reversible change to one image definition: added, removed, replaced.
pub type ImageDefChange = TableChange<ImageDef>;

/// A new drawing's image definitions: none.
pub(crate) fn initial_image_defs() -> ImageDefTable {
    Table::empty()
}

/// A 64-bit FNV-1a fingerprint of `bytes`: what names a picture in a file
/// and finds the same one attached twice. Not a security measure.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The format and pixel size `bytes` say they are, refused unless they are
/// a picture the host makes.
fn header(bytes: &[u8]) -> Result<(ImageFormat, u32, u32), ImageDefError> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        // The first chunk is IHDR: width, height, bit depth, colour type,
        // compression, filter, interlace.
        let ihdr = bytes.get(8..33).ok_or(ImageDefError::NotAnImage)?;
        if &ihdr[4..8] != b"IHDR" {
            return Err(ImageDefError::NotAnImage);
        }
        let word =
            |at: usize| u32::from_be_bytes([ihdr[at], ihdr[at + 1], ihdr[at + 2], ihdr[at + 3]]);
        let (width, height) = (word(8), word(12));
        let (depth, colour, interlace) = (ihdr[16], ihdr[17], ihdr[20]);
        if depth != 8 {
            return Err(ImageDefError::Unsupported("a PNG not of 8 bits a sample"));
        }
        match colour {
            0 | 2 => {}
            3 => return Err(ImageDefError::Unsupported("a PNG with a palette")),
            _ => return Err(ImageDefError::Unsupported("a PNG with transparency")),
        }
        if interlace != 0 {
            return Err(ImageDefError::Unsupported("an interlaced PNG"));
        }
        return Ok((ImageFormat::Png, width, height));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // Markers to the first start of frame: its precision, height,
        // width, and components.
        let mut at = 2;
        loop {
            let marker = bytes.get(at..at + 2).ok_or(ImageDefError::NotAnImage)?;
            if marker[0] != 0xFF {
                return Err(ImageDefError::NotAnImage);
            }
            let kind = marker[1];
            // Fill bytes before a marker.
            if kind == 0xFF {
                at += 1;
                continue;
            }
            let length = bytes.get(at + 2..at + 4).ok_or(ImageDefError::NotAnImage)?;
            let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
            let is_frame = matches!(kind, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
            if is_frame {
                let frame = bytes
                    .get(at + 4..at + 10)
                    .ok_or(ImageDefError::NotAnImage)?;
                let height = u32::from(u16::from_be_bytes([frame[1], frame[2]]));
                let width = u32::from(u16::from_be_bytes([frame[3], frame[4]]));
                return match frame[5] {
                    1 | 3 => Ok((ImageFormat::Jpeg, width, height)),
                    _ => Err(ImageDefError::Unsupported("a JPEG neither grey nor colour")),
                };
            }
            if length < 2 {
                return Err(ImageDefError::NotAnImage);
            }
            at += 2 + length;
        }
    }
    Err(ImageDefError::NotAnImage)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG's signature and IHDR, of `width` × `height`, grey or colour:
    /// all a definition reads.
    pub(crate) fn png(width: u32, height: u32, colour: u8) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, colour, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    /// A JPEG's start, an application segment, and a start of frame.
    pub(crate) fn jpeg(width: u16, height: u16, components: u8) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46];
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[components, 1, 0x11, 0]);
        bytes
    }

    #[test]
    fn a_definition_reads_its_size_from_its_header() {
        let def = ImageDef::new("plan", png(640, 480, 2)).unwrap();
        assert_eq!(
            (def.format, def.width, def.height),
            (ImageFormat::Png, 640, 480)
        );
        assert_eq!(def.digest, fingerprint(&png(640, 480, 2)));
        def.check().unwrap();
        let photo = ImageDef::new("sketch", jpeg(2048, 1536, 3)).unwrap();
        assert_eq!(
            (photo.format, photo.width, photo.height),
            (ImageFormat::Jpeg, 2048, 1536)
        );
        assert_eq!(ImageDef::new("grey", jpeg(10, 10, 1)).unwrap().width, 10);
    }

    #[test]
    fn what_the_host_does_not_make_is_refused() {
        let refused = |bytes: Vec<u8>| ImageDef::new("x", bytes).unwrap_err();
        assert_eq!(refused(b"GIF89a".to_vec()), ImageDefError::NotAnImage);
        assert_eq!(
            refused(png(10, 10, 6)),
            ImageDefError::Unsupported("a PNG with transparency")
        );
        assert_eq!(
            refused(png(10, 10, 3)),
            ImageDefError::Unsupported("a PNG with a palette")
        );
        let mut interlaced = png(10, 10, 2);
        interlaced[28] = 1;
        assert_eq!(
            refused(interlaced),
            ImageDefError::Unsupported("an interlaced PNG")
        );
        let mut sixteen = png(10, 10, 2);
        sixteen[24] = 16;
        assert_eq!(
            refused(sixteen),
            ImageDefError::Unsupported("a PNG not of 8 bits a sample")
        );
        assert_eq!(
            refused(jpeg(10, 10, 4)),
            ImageDefError::Unsupported("a JPEG neither grey nor colour")
        );
        assert_eq!(
            refused(png(9000, 10, 2)),
            ImageDefError::BadSize {
                width: 9000,
                height: 10
            }
        );
        assert_eq!(
            refused(png(0, 10, 2)),
            ImageDefError::BadSize {
                width: 0,
                height: 10
            }
        );
        assert_eq!(
            refused(png(10, 10, 2)[..20].to_vec()),
            ImageDefError::NotAnImage
        );
        assert_eq!(
            refused(jpeg(10, 10, 3)[..12].to_vec()),
            ImageDefError::NotAnImage
        );
        assert!(matches!(
            ImageDef::new("", png(1, 1, 0)),
            Err(ImageDefError::BadName(_))
        ));
        assert_eq!(
            ImageDef::new("x", vec![0; MOST_BYTES + 1]).unwrap_err(),
            ImageDefError::TooLarge(MOST_BYTES + 1)
        );
    }

    #[test]
    fn a_missing_picture_keeps_its_name_and_size() {
        let def = ImageDef::missing("site.tif", 1200, 800).unwrap();
        assert!(def.is_missing());
        assert_eq!((def.width, def.height, def.digest), (1200, 800, 0));
        def.check().unwrap();
        assert!(ImageDef::missing("x", 0, 10).is_err());
        assert!(ImageDef::missing("", 10, 10).is_err());
    }

    #[test]
    fn a_row_described_otherwise_than_its_bytes_is_refused() {
        let mut def = ImageDef::new("plan", png(640, 480, 2)).unwrap();
        def.width = 641;
        assert!(def.check().is_err());
    }

    proptest::proptest! {
        /// Any bytes at all: a definition or a refusal, never a panic.
        #[test]
        fn any_bytes_are_read_or_refused(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..64)) {
            let _ = ImageDef::new("x", bytes.clone());
            let mut png_start = b"\x89PNG\r\n\x1a\n".to_vec();
            png_start.extend(&bytes);
            let _ = ImageDef::new("x", png_start);
            let mut jpeg_start = vec![0xFF, 0xD8];
            jpeg_start.extend(&bytes);
            let _ = ImageDef::new("x", jpeg_start);
        }
    }
}
