//! Reading a WIPEOUT (#226).
//!
//! A WIPEOUT is an IMAGE with no picture: an image of `13, 23` pixels,
//! placed by the corner of its first pixel (10, 20) and the vectors of a
//! pixel across (11, 21) and up (12, 22), clipped by a boundary (71: 1 a
//! rectangle by two corners, 2 a polygon) of corners in pixel coordinates
//! (14, 24), whose y runs down the picture. A writer gives it one pixel,
//! so the corners run from −½ to ½. Without a boundary it is the whole
//! image. Its corners are read into the drawing's coordinates.

use cad_core::{Point2, Vec2};
use cad_document::WipeoutEntity;

use crate::import::INVALID;
use crate::spline::Groups;

/// A boundary that masks what is outside it (290): not held.
pub(crate) const INVERTED: &str = "WIPEOUT (inverted)";

/// The wipeout the groups describe, or the report key of why it is not
/// one.
pub(crate) fn read(raw: &[(i32, String)]) -> Result<WipeoutEntity, &'static str> {
    let groups = Groups(raw);
    if groups.int(290).is_some_and(|inverted| inverted != 0) {
        return Err(INVERTED);
    }
    let first = |x: i32| groups.points(x).first().copied();
    let corner = first(10).ok_or(INVALID)?;
    let across = first(11).ok_or(INVALID)? - Point2::ORIGIN;
    let up = first(12).ok_or(INVALID)? - Point2::ORIGIN;
    let size = first(13).map_or(Vec2::new(1.0, 1.0), |size| size - Point2::ORIGIN);
    // Pixel coordinates — x right, y down from the top row — into the
    // drawing's.
    let place = |pixel: Point2| corner + across * (pixel.x + 0.5) + up * (size.y - (pixel.y + 0.5));
    let (low, high) = (
        Point2::new(-0.5, -0.5),
        Point2::new(size.x - 0.5, size.y - 0.5),
    );
    let mut pixels = groups.points(14);
    if groups.int(71) == Some(1) || pixels.len() < 3 {
        let (a, b) = match pixels.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => (low, high),
        };
        pixels = vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)];
    }
    WipeoutEntity::new(pixels.into_iter().map(place).collect()).map_err(|_| INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(groups: &[(i32, &str)]) -> Vec<(i32, String)> {
        groups
            .iter()
            .map(|(code, value)| (*code, (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_polygon_in_pixel_coordinates_lands_on_its_box() {
        let wipeout = read(&raw(&[
            (10, "60"),
            (20, "-5"),
            (11, "30"),
            (21, "0"),
            (12, "0"),
            (22, "10"),
            (13, "1"),
            (23, "1"),
            (71, "2"),
            (91, "4"),
            (14, "-0.5"),
            (24, "0.5"),
            (14, "0.5"),
            (24, "0.5"),
            (14, "0"),
            (24, "-0.5"),
            (14, "-0.5"),
            (24, "0.5"),
        ]))
        .unwrap();
        assert_eq!(
            wipeout.vertices,
            [
                Point2::new(60.0, -5.0),
                Point2::new(90.0, -5.0),
                Point2::new(75.0, 5.0)
            ]
        );
    }

    #[test]
    fn a_rectangle_or_no_boundary_is_the_box_and_an_inverted_one_is_reported() {
        let placed = [
            (10, "0"),
            (20, "0"),
            (11, "4"),
            (21, "0"),
            (12, "0"),
            (22, "2"),
        ];
        let whole = read(&raw(&placed)).unwrap();
        assert_eq!(whole.bounds().max(), Point2::new(4.0, 2.0));
        let mut rectangle = placed.to_vec();
        rectangle.extend([(71, "1"), (14, "-0.5"), (24, "-0.5"), (14, "0"), (24, "0")]);
        let half = read(&raw(&rectangle)).unwrap();
        assert_eq!(half.bounds().min(), Point2::new(0.0, 1.0));
        assert_eq!(half.bounds().max(), Point2::new(2.0, 2.0));
        let mut inverted = placed.to_vec();
        inverted.push((290, "1"));
        assert_eq!(read(&raw(&inverted)), Err(INVERTED));
        assert_eq!(read(&raw(&placed[..2])), Err(INVALID));
    }
}
