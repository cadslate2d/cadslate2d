//! Images (ADR-0042 §1): a picture placed under the drawing.
//!
//! An image shows a definition (`image_def`) as a parallelogram: its
//! insertion point is the picture's lower-left corner, and `u` and `v` are
//! one pixel's width and height in drawing units, as DXF's `IMAGE` has
//! them, so any affine transform maps an image to an image. It carries the
//! picture's size in pixels too, as DXF's does, so its geometry needs no
//! table: a definition reloaded at another size changes its images in the
//! same transaction.
//!
//! Its geometry is its frame. Picking, snapping to its corners, its bounds,
//! every transform, and its grips are a closed four-edge outline's; the
//! picture is not geometry, and nothing snaps to what it shows. Its fill —
//! the picture — is drawn beneath strokes in its record's tier (§2).
//! Whether its frame is drawn is the drawing's setting, `IMAGEFRAME`, as
//! in DXF.

use std::fmt;

use cad_core::tolerance::AngularTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Primitive, Segment, Transform};

use crate::image_def::ImageDefId;

/// How faded an image is drawn at most, in percent, as AutoCAD's.
pub const MOST_FADE: u8 = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct ImageEntity {
    /// The definition it shows.
    pub def: ImageDefId,
    /// Its lower-left corner.
    pub insertion: Point2,
    /// One pixel's width, along the picture's bottom edge.
    pub u: Vec2,
    /// One pixel's height, along its left edge.
    pub v: Vec2,
    /// The picture's size in pixels.
    pub pixels: [u32; 2],
    /// How faded it is drawn, 0–[`MOST_FADE`] percent.
    pub fade: u8,
}

/// Why an image cannot be placed so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// A coordinate is not finite.
    Position,
    /// Its pixel vectors are of no length, or parallel: it covers nothing.
    NoArea,
    /// Its picture has no pixels.
    NoPixels,
    /// More faded than [`MOST_FADE`].
    Fade,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position => write!(f, "an image's corners must be finite"),
            Self::NoArea => write!(f, "an image must cover an area"),
            Self::NoPixels => write!(f, "an image's picture must have pixels"),
            Self::Fade => write!(f, "an image's fade is at most {MOST_FADE}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl ImageEntity {
    /// An image of `def`, `pixels` in size, its lower-left corner at
    /// `insertion`, a pixel `u` across and `v` up; unfaded.
    pub fn new(
        def: ImageDefId,
        insertion: Point2,
        u: Vec2,
        v: Vec2,
        pixels: [u32; 2],
    ) -> Result<Self, ImageError> {
        let image = Self {
            def,
            insertion,
            u,
            v,
            pixels,
            fade: 0,
        };
        image.check()?;
        Ok(image)
    }

    /// An image `width` drawing units across, upright, square pixels.
    pub fn placed(
        def: ImageDefId,
        insertion: Point2,
        width: f64,
        pixels: [u32; 2],
    ) -> Result<Self, ImageError> {
        let size = width / f64::from(pixels[0].max(1));
        Self::new(
            def,
            insertion,
            Vec2::new(size, 0.0),
            Vec2::new(0.0, size),
            pixels,
        )
    }

    /// The same frame showing a picture of `pixels` pixels: a definition
    /// read or reloaded at another size keeps where the image is.
    pub fn refitted(&self, pixels: [u32; 2]) -> Self {
        let mut image = self.clone();
        if image.pixels != pixels && pixels[0] > 0 && pixels[1] > 0 {
            image.u = self.across() * (1.0 / f64::from(pixels[0]));
            image.v = self.up() * (1.0 / f64::from(pixels[1]));
            image.pixels = pixels;
        }
        image
    }

    /// Whether it can be drawn: finite, covering an area, of some pixels,
    /// faded no more than allowed.
    pub fn check(&self) -> Result<(), ImageError> {
        let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        if !self.insertion.is_finite() || !finite(self.u) || !finite(self.v) {
            return Err(ImageError::Position);
        }
        if self.pixels[0] == 0 || self.pixels[1] == 0 {
            return Err(ImageError::NoPixels);
        }
        let (across, up) = (self.across(), self.up());
        let lengths = across.length() * up.length();
        // Empty or parallel edges cover nothing: the sine of the angle
        // between them is no more than an angular tolerance.
        let sine = (across.x * up.y - across.y * up.x) / lengths;
        if lengths == 0.0 || !lengths.is_finite() || AngularTolerance::DEFAULT.is_zero(sine) {
            return Err(ImageError::NoArea);
        }
        if self.fade > MOST_FADE {
            return Err(ImageError::Fade);
        }
        Ok(())
    }

    /// The picture's bottom edge, as a vector.
    pub fn across(&self) -> Vec2 {
        self.u * f64::from(self.pixels[0])
    }

    /// Its left edge, as a vector.
    pub fn up(&self) -> Vec2 {
        self.v * f64::from(self.pixels[1])
    }

    /// Its corners: lower-left, lower-right, upper-right, upper-left.
    pub fn corners(&self) -> [Point2; 4] {
        let (across, up) = (self.across(), self.up());
        [
            self.insertion,
            self.insertion + across,
            self.insertion + across + up,
            self.insertion + up,
        ]
    }

    /// Its frame's edges, from the lower-left corner round.
    pub fn edges(&self) -> Vec<Segment> {
        let c = self.corners();
        (0..4).map(|i| Segment::new(c[i], c[(i + 1) % 4])).collect()
    }

    pub fn primitives(&self) -> Vec<Primitive> {
        self.edges().into_iter().map(Primitive::Segment).collect()
    }

    pub fn bounds(&self) -> Aabb {
        let c = self.corners();
        c[1..].iter().fold(Aabb::from_points(c[0], c[0]), |b, p| {
            b.union(&Aabb::from_points(*p, *p))
        })
    }

    /// The distance to its frame: an image is picked by its frame.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.edges()
            .iter()
            .map(|edge| edge.distance_to_point(point))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether `point` is on the picture.
    pub fn contains(&self, point: Point2) -> bool {
        cad_geometry::ring_contains(&self.corners(), point)
    }

    /// The image through `transform`: its corner moved, its pixel vectors
    /// mapped as the plane is. A reflection reverses the picture, as it
    /// would on paper.
    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let at = transform.apply(self.insertion);
        Self {
            insertion: at,
            u: transform.apply(self.insertion + self.u) - at,
            v: transform.apply(self.insertion + self.v) - at,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::TableId;

    fn def() -> ImageDefId {
        TableId::from_raw(1)
    }

    #[test]
    fn an_image_is_placed_by_its_corner_and_a_pixels_vectors() {
        let image = ImageEntity::placed(def(), Point2::new(10.0, 20.0), 200.0, [400, 300]).unwrap();
        assert_eq!(
            image.corners(),
            [
                Point2::new(10.0, 20.0),
                Point2::new(210.0, 20.0),
                Point2::new(210.0, 170.0),
                Point2::new(10.0, 170.0)
            ]
        );
        assert_eq!(image.bounds().max(), Point2::new(210.0, 170.0));
        assert_eq!(image.primitives().len(), 4);
        assert!((image.distance_to(Point2::new(110.0, 25.0)) - 5.0).abs() < 1e-12);
        assert!(image.contains(Point2::new(110.0, 25.0)));
        assert!(!image.contains(Point2::new(300.0, 25.0)));
    }

    #[test]
    fn an_image_that_covers_nothing_is_refused() {
        let at = Point2::ORIGIN;
        let x = Vec2::new(1.0, 0.0);
        assert_eq!(
            ImageEntity::new(def(), at, x, x, [10, 10]),
            Err(ImageError::NoArea)
        );
        assert_eq!(
            ImageEntity::new(def(), at, x, Vec2::new(0.0, 0.0), [10, 10]),
            Err(ImageError::NoArea)
        );
        assert_eq!(
            ImageEntity::new(def(), at, x, Vec2::new(0.0, 1.0), [0, 10]),
            Err(ImageError::NoPixels)
        );
        assert_eq!(
            ImageEntity::new(
                def(),
                Point2::new(f64::NAN, 0.0),
                x,
                Vec2::new(0.0, 1.0),
                [1, 1]
            ),
            Err(ImageError::Position)
        );
        let mut faded = ImageEntity::placed(def(), at, 10.0, [10, 10]).unwrap();
        faded.fade = 101;
        assert_eq!(faded.check(), Err(ImageError::Fade));
    }

    proptest::proptest! {
        /// Any move, turn, scaling, or reflection maps an image's corners
        /// as it maps the points.
        #[test]
        fn a_transform_maps_its_corners(
            kind in 0u8..4, angle in -720.0f64..720.0, factor in 0.01f64..100.0,
            dx in -1e4f64..1e4, dy in -1e4f64..1e4,
        ) {
            let image = ImageEntity::placed(def(), Point2::new(3.0, -7.0), 120.0, [64, 48]).unwrap();
            let about = Point2::new(dx, dy);
            let transform = match kind {
                0 => Transform::translation(Vec2::new(dx, dy)),
                1 => Transform::rotation(about, angle),
                2 => Transform::scaling(about, factor).unwrap(),
                _ => Transform::mirror(about, about + Vec2::new(angle.cos(), angle.sin())).unwrap(),
            };
            let moved = image.transformed(&transform);
            moved.check().unwrap();
            for (corner, expected) in moved.corners().iter().zip(image.corners()) {
                let expected = transform.apply(expected);
                let tolerance = 1e-9 * (1.0 + expected.x.abs().max(expected.y.abs()));
                proptest::prop_assert!((corner.x - expected.x).abs() <= tolerance);
                proptest::prop_assert!((corner.y - expected.y).abs() <= tolerance);
            }
        }
    }
}
