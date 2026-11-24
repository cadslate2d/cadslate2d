//! Images (ADR-0042 §4): an `IMAGE` entity, the `IMAGEDEF` object it
//! names, and our copy of the picture's bytes.
//!
//! An IMAGE places a picture of `13, 23` pixels by its lower-left corner
//! (10, 20) and the vectors of one pixel across (11, 21) and up (12, 22),
//! as a WIPEOUT does, naming its IMAGEDEF by handle (340). The IMAGEDEF
//! names a file (1) and gives its size in pixels (10, 20); the file itself
//! is outside the DXF, where a browser cannot open it by path. So the
//! writer puts a copy of the bytes in an `XRECORD` of the `CADSLATE_IMAGES`
//! dictionary — the IMAGEDEF's handle (340), its size in bytes (90), then
//! the bytes as binary chunks (310) — and a DXF from CadSlate2D is whole in
//! one file. Other programs pass the dictionary over and look for the
//! file; a DXF from one of them has no copy, and its pictures are missing
//! until reloaded.

use cad_core::Point2;
use cad_document::{ImageDefId, ImageEntity, MOST_FADE};

use crate::import::INVALID;
use crate::spline::Groups;

/// The dictionary that holds our copies of the pictures.
pub const IMAGES_DICTIONARY: &str = "CADSLATE_IMAGES";

/// An image clipped to a boundary of its own (71, 14, 24): drawn whole.
pub(crate) const CLIPPED: &str = "IMAGE (clipping boundary)";
/// An image's brightness or contrast (281, 282) set otherwise than 50.
pub(crate) const ADJUSTED: &str = "IMAGE (brightness or contrast)";
/// An image naming a definition the file does not have.
pub(crate) const UNDEFINED: &str = "IMAGE (no definition)";
/// A copy of a picture larger than a definition holds, or that is not one.
pub(crate) const UNREADABLE: &str = "IMAGE (picture not readable)";

/// What an IMAGE's groups say: the image, its definition's handle, and
/// what it held that the drawing does not.
pub(crate) struct Read {
    pub image: ImageEntity,
    pub def: String,
    pub dropped: Vec<&'static str>,
}

/// The image the groups describe, its definition named by handle and to be
/// resolved once the objects are read; or the report key of why it is
/// not one.
pub(crate) fn read(raw: &[(i32, String)]) -> Result<Read, &'static str> {
    let groups = Groups(raw);
    let first = |x: i32| groups.points(x).first().copied();
    let corner = first(10).ok_or(INVALID)?;
    let u = first(11).ok_or(INVALID)? - Point2::ORIGIN;
    let v = first(12).ok_or(INVALID)? - Point2::ORIGIN;
    let size = first(13).ok_or(INVALID)? - Point2::ORIGIN;
    let pixel = |value: f64| {
        (value.is_finite() && value >= 1.0 && value <= f64::from(u32::MAX))
            .then(|| value.round() as u32)
    };
    let pixels = [pixel(size.x).ok_or(INVALID)?, pixel(size.y).ok_or(INVALID)?];
    let def = raw
        .iter()
        .find(|(code, _)| *code == 340)
        .map(|(_, handle)| handle.trim().to_uppercase())
        .ok_or(UNDEFINED)?;
    let mut image =
        ImageEntity::new(ImageDefId::FIRST, corner, u, v, pixels).map_err(|_| INVALID)?;
    image.fade = groups
        .int(283)
        .map_or(0, |fade| fade.clamp(0, i64::from(MOST_FADE)) as u8);
    let mut dropped = Vec::new();
    // Clipped (280) to a boundary that is not the whole picture.
    if groups.int(280).is_some_and(|clipped| clipped != 0) {
        let corners = groups.points(14);
        let whole = |p: &Point2, q: Point2| (p.x - q.x).abs() < 0.5 && (p.y - q.y).abs() < 0.5;
        let full = corners.len() == 2
            && whole(&corners[0], Point2::new(-0.5, -0.5))
            && whole(&corners[1], Point2::new(size.x - 0.5, size.y - 0.5));
        if !full {
            dropped.push(CLIPPED);
        }
    }
    if groups.int(281).is_some_and(|b| b != 50) || groups.int(282).is_some_and(|c| c != 50) {
        dropped.push(ADJUSTED);
    }
    Ok(Read {
        image,
        def,
        dropped,
    })
}

/// The bytes of our copy of a picture: the 310 chunks of the XRECORD, as
/// hexadecimal, at most `most` of them — `MOST_BYTES`, what a definition
/// holds, from a file; `None` when a chunk is not hexadecimal or there are
/// more. Reading stops there: a copy is never gathered past the bound.
pub(crate) fn copied_bytes(raw: &[(i32, String)], most: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    for (_, chunk) in raw.iter().filter(|(code, _)| *code == 310) {
        let chunk = chunk.trim().as_bytes();
        if chunk.len() % 2 != 0 || bytes.len() + chunk.len() / 2 > most {
            return None;
        }
        let (pairs, _) = chunk.as_chunks::<2>();
        for [high, low] in pairs {
            let digit = |c: u8| (c as char).to_digit(16);
            bytes.push((digit(*high)? * 16 + digit(*low)?) as u8);
        }
    }
    Some(bytes)
}

/// A picture's bytes as the 310 chunks of its copy: 127 bytes each, as a
/// binary chunk of DXF holds, in upper-case hexadecimal.
pub(crate) fn chunks(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes.chunks(127).map(|chunk| {
        let mut text = String::with_capacity(chunk.len() * 2);
        for byte in chunk {
            text.push_str(&format!("{byte:02X}"));
        }
        text
    })
}

/// A picture's file name, as an IMAGEDEF gives it (1): its last part, as
/// a browser has no path to it.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim()
}

/// The vectors of one pixel of an image `width` across, upright: square
/// pixels.
#[cfg(test)]
fn square(width: f64, pixels: u32) -> (cad_core::Vec2, cad_core::Vec2) {
    let size = width / f64::from(pixels);
    (
        cad_core::Vec2::new(size, 0.0),
        cad_core::Vec2::new(0.0, size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_document::MOST_BYTES;

    fn raw(groups: &[(i32, &str)]) -> Vec<(i32, String)> {
        groups
            .iter()
            .map(|(code, value)| (*code, (*value).to_owned()))
            .collect()
    }

    /// The IMAGE ezdxf 1.4.4 writes for a 64 × 48 picture 640 units wide
    /// at (10, 20).
    fn ezdxf() -> Vec<(i32, String)> {
        raw(&[
            (100, "AcDbRasterImage"),
            (90, "0"),
            (10, "10.0"),
            (20, "20.0"),
            (30, "0.0"),
            (11, "10.0"),
            (21, "0.0"),
            (31, "0.0"),
            (12, "6.123233995736766e-16"),
            (22, "10.0"),
            (32, "0.0"),
            (13, "64.0"),
            (23, "48.0"),
            (340, "31"),
            (70, "3"),
            (280, "0"),
            (281, "50"),
            (282, "50"),
            (283, "0"),
            (360, "33"),
            (71, "1"),
            (91, "2"),
            (14, "-0.5"),
            (24, "-0.5"),
            (14, "63.5"),
            (24, "47.5"),
            (290, "0"),
        ])
    }

    #[test]
    fn an_image_is_read_by_its_corner_its_pixel_vectors_and_its_definition() {
        let read = read(&ezdxf()).unwrap();
        assert_eq!(read.def, "31");
        assert!(read.dropped.is_empty());
        assert_eq!(read.image.insertion, Point2::new(10.0, 20.0));
        assert_eq!(read.image.pixels, [64, 48]);
        assert_eq!(read.image.across(), cad_core::Vec2::new(640.0, 0.0));
        let (u, _) = square(640.0, 64);
        assert_eq!(read.image.u, u);
    }

    #[test]
    fn what_an_image_holds_that_the_drawing_does_not_is_reported() {
        let mut clipped = ezdxf();
        for (code, value) in &mut clipped {
            match code {
                280 => *value = "1".into(),
                281 => *value = "70".into(),
                _ => {}
            }
        }
        // A boundary inside the picture.
        clipped.retain(|(code, _)| !matches!(code, 14 | 24));
        clipped.extend(raw(&[(14, "5.5"), (24, "5.5"), (14, "20.5"), (24, "20.5")]));
        let read = read(&clipped).unwrap();
        assert_eq!(read.dropped, [CLIPPED, ADJUSTED]);
        let mut undefined = ezdxf();
        undefined.retain(|(code, _)| *code != 340);
        assert_eq!(super::read(&undefined).err(), Some(UNDEFINED));
        let mut flat = ezdxf();
        flat.retain(|(code, _)| !matches!(code, 12 | 22 | 32));
        assert_eq!(super::read(&flat).err(), Some(INVALID));
    }

    #[test]
    fn a_picture_is_copied_in_chunks_and_read_back() {
        let bytes: Vec<u8> = (0..=255).cycle().take(300).collect();
        let chunks: Vec<String> = chunks(&bytes).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 254);
        let raw: Vec<(i32, String)> = chunks.into_iter().map(|c| (310, c)).collect();
        assert_eq!(copied_bytes(&raw, MOST_BYTES).unwrap(), bytes);
        assert_eq!(copied_bytes(&[(310, "4G".to_owned())], MOST_BYTES), None);
        assert_eq!(copied_bytes(&[(310, "ABC".to_owned())], MOST_BYTES), None);
        // The bound: a copy of more bytes than it is read.
        assert_eq!(copied_bytes(&raw, 299), None);
        assert_eq!(copied_bytes(&raw, 300).unwrap().len(), 300);
    }

    #[test]
    fn a_definition_of_another_size_keeps_the_frame() {
        let image = read(&ezdxf()).unwrap().image;
        let fitted = image.refitted([128, 96]);
        assert_eq!(fitted.pixels, [128, 96]);
        assert!((fitted.across() - image.across()).length() < 1e-9);
        assert!((fitted.up() - image.up()).length() < 1e-9);
        assert_eq!(file_name("C:\\images\\plan.png"), "plan.png");
        assert_eq!(file_name("sketches/site.jpg"), "site.jpg");
    }
}
