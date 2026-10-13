//! Viewports (#192; ADR-0030): a window on paper onto model space.
//!
//! A viewport is a rectangle on a layout's paper — its centre, width, and
//! height in paper units — and the model's view it shows: the model point
//! at its centre and the scale, paper units per model unit. It is drawn as
//! its frame, and its contents are the model's committed scene drawn again
//! under its own transform, clipped to the frame (ADR-0030). It stays
//! upright: a turn or a mirror moves its centre and leaves its frame
//! square to the paper.

use std::fmt;

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Segment, Transform};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportEntity {
    /// The centre of the frame, on paper.
    pub center: Point2,
    /// The frame's width and height, in paper units; positive.
    pub width: f64,
    pub height: f64,
    /// The model point shown at the frame's centre.
    pub view_center: Point2,
    /// Paper units per model unit: `0.02` is 1:50. Positive.
    pub scale: f64,
    /// Whether the model shows through; off, only the frame is drawn.
    pub on: bool,
}

/// Why a viewport cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportError {
    /// The frame's width or height is not a positive, finite number.
    Frame,
    /// The scale is not a positive, finite number.
    Scale,
    /// A coordinate is not finite.
    Position,
}

impl fmt::Display for ViewportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame => write!(f, "a viewport's width and height must be positive"),
            Self::Scale => write!(f, "a viewport's scale must be positive"),
            Self::Position => write!(f, "a viewport's position must be finite"),
        }
    }
}

impl std::error::Error for ViewportError {}

impl ViewportEntity {
    pub fn new(center: Point2, width: f64, height: f64, view_center: Point2, scale: f64) -> Self {
        Self {
            center,
            width,
            height,
            view_center,
            scale,
            on: true,
        }
    }

    /// A viewport over `frame` showing `view` — the model's extents — at
    /// the largest scale that fits it, centred; a unit a unit when there
    /// is nothing to show.
    pub fn fitting(frame: &Aabb, view: Option<&Aabb>) -> Self {
        let size = frame.max() - frame.min();
        let (view_center, scale) = match view {
            Some(view) => {
                let extent = view.max() - view.min();
                let fit_x = if extent.x > 0.0 {
                    size.x / extent.x
                } else {
                    f64::INFINITY
                };
                let fit_y = if extent.y > 0.0 {
                    size.y / extent.y
                } else {
                    f64::INFINITY
                };
                let fit = fit_x.min(fit_y);
                (view.center(), if fit.is_finite() { fit } else { 1.0 })
            }
            None => (Point2::ORIGIN, 1.0),
        };
        Self::new(frame.center(), size.x, size.y, view_center, scale)
    }

    pub fn check(&self) -> Result<(), ViewportError> {
        let finite = |point: Point2| point.x.is_finite() && point.y.is_finite();
        if !(finite(self.center) && finite(self.view_center)) {
            return Err(ViewportError::Position);
        }
        if !(self.width.is_finite()
            && self.width > 0.0
            && self.height.is_finite()
            && self.height > 0.0)
        {
            return Err(ViewportError::Frame);
        }
        if !(self.scale.is_finite() && self.scale > 0.0) {
            return Err(ViewportError::Scale);
        }
        Ok(())
    }

    /// The frame on paper.
    pub fn frame(&self) -> Aabb {
        let half = Vec2::new(self.width / 2.0, self.height / 2.0);
        Aabb::from_points(self.center - half, self.center + half)
    }

    /// The frame's corners: bottom left, bottom right, top right, top left.
    pub fn corners(&self) -> [Point2; 4] {
        self.frame().corners()
    }

    pub fn edges(&self) -> [Segment; 4] {
        let [a, b, c, d] = self.corners();
        [
            Segment::new(a, b),
            Segment::new(b, c),
            Segment::new(c, d),
            Segment::new(d, a),
        ]
    }

    /// The height of the model's view through the frame, in model units:
    /// what DXF stores (45).
    pub fn view_height(&self) -> f64 {
        self.height / self.scale
    }

    /// Whether `point` on paper is inside the frame.
    pub fn contains(&self, point: Point2) -> bool {
        self.frame().contains_point(point)
    }

    /// The distance from `point` to the frame.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.edges()
            .iter()
            .map(|edge| edge.distance_to_point(point))
            .fold(f64::INFINITY, f64::min)
    }

    /// The model point shown at `paper`, a point on the paper.
    pub fn model_point(&self, paper: Point2) -> Point2 {
        self.view_center + (paper - self.center) * (1.0 / self.scale)
    }

    /// Where the model point `model` lands on the paper.
    pub fn paper_point(&self, model: Point2) -> Point2 {
        self.center + (model - self.view_center) * self.scale
    }

    /// The viewport moved, scaled, turned, or mirrored: its centre goes
    /// where the transform takes it and its frame scales with it, but
    /// stays square to the paper, as a sheet's windows do.
    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let factor = transform.scale_factor();
        Self {
            center: transform.apply(self.center),
            width: self.width * factor,
            height: self.height * factor,
            ..*self
        }
    }

    /// The viewport with corner `index` (as [`corners`](Self::corners)
    /// orders them) at `to`, the opposite corner staying: a resize.
    /// `None` for a frame that would have no width or height.
    #[must_use]
    pub fn with_corner(&self, index: usize, to: Point2) -> Option<Self> {
        let corners = self.corners();
        let opposite = *corners.get((index + 2) % 4)?;
        if index >= 4 {
            return None;
        }
        let frame = Aabb::from_points(opposite, to);
        let size = frame.max() - frame.min();
        let tolerance = LinearTolerance::DEFAULT.around(&[opposite, to]);
        if tolerance.is_zero(size.x) || tolerance.is_zero(size.y) {
            return None;
        }
        Some(Self {
            center: frame.center(),
            width: size.x,
            height: size.y,
            ..*self
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn viewport() -> ViewportEntity {
        ViewportEntity::new(p(100.0, 50.0), 200.0, 100.0, p(1000.0, 500.0), 0.5)
    }

    #[test]
    fn the_frame_is_the_centre_and_size_and_its_view_maps_both_ways() {
        let v = viewport();
        assert_eq!(v.frame(), Aabb::from_points(p(0.0, 0.0), p(200.0, 100.0)));
        assert_eq!(v.corners()[0], p(0.0, 0.0));
        assert_eq!(v.corners()[2], p(200.0, 100.0));
        assert_eq!(v.view_height(), 200.0);
        assert_eq!(v.model_point(p(100.0, 50.0)), p(1000.0, 500.0));
        assert_eq!(v.model_point(p(0.0, 0.0)), p(800.0, 400.0));
        assert_eq!(v.paper_point(p(800.0, 400.0)), p(0.0, 0.0));
        assert!(v.contains(p(10.0, 10.0)));
        assert!(!v.contains(p(-1.0, 10.0)));
        assert_eq!(v.distance_to(p(100.0, -5.0)), 5.0);
        assert_eq!(
            v.distance_to(p(100.0, 50.0)),
            50.0,
            "the frame, not the inside"
        );
    }

    #[test]
    fn fitting_shows_the_view_at_the_scale_that_fits_and_a_unit_a_unit_for_nothing() {
        let frame = Aabb::from_points(p(0.0, 0.0), p(200.0, 100.0));
        let view = Aabb::from_points(p(0.0, 0.0), p(1000.0, 1000.0));
        let v = ViewportEntity::fitting(&frame, Some(&view));
        assert_eq!(v.center, p(100.0, 50.0));
        assert_eq!(v.view_center, p(500.0, 500.0));
        assert_eq!(v.scale, 0.1, "the height decides");
        let empty = ViewportEntity::fitting(&frame, None);
        assert_eq!(empty.scale, 1.0);
        assert_eq!(empty.view_center, Point2::ORIGIN);
        let point = Aabb::from_points(p(5.0, 5.0), p(5.0, 5.0));
        assert_eq!(ViewportEntity::fitting(&frame, Some(&point)).scale, 1.0);
    }

    #[test]
    fn a_corner_moved_resizes_about_the_opposite_one_and_a_flat_frame_is_refused() {
        let v = viewport();
        let moved = v.with_corner(2, p(300.0, 150.0)).unwrap();
        assert_eq!(
            moved.frame(),
            Aabb::from_points(p(0.0, 0.0), p(300.0, 150.0))
        );
        assert_eq!(moved.scale, v.scale, "the view is untouched");
        let past = v.with_corner(0, p(250.0, 120.0)).unwrap();
        assert_eq!(
            past.frame(),
            Aabb::from_points(p(200.0, 100.0), p(250.0, 120.0))
        );
        assert!(v.with_corner(0, p(200.0, 10.0)).is_none());
        assert!(v.with_corner(4, p(1.0, 1.0)).is_none());
    }

    #[test]
    fn a_transform_moves_and_scales_the_frame_but_keeps_it_square() {
        let v = viewport();
        let turned = v.transformed(&Transform::rotation(Point2::ORIGIN, 90.0));
        assert!((turned.center.x - -50.0).abs() < 1e-9 && (turned.center.y - 100.0).abs() < 1e-9);
        assert_eq!((turned.width, turned.height), (200.0, 100.0));
        let scaled = v.transformed(&Transform::scaling(Point2::ORIGIN, 2.0).unwrap());
        assert_eq!((scaled.width, scaled.height), (400.0, 200.0));
        assert_eq!(scaled.center, p(200.0, 100.0));
        assert_eq!(
            scaled.scale, 0.5,
            "the view's scale is the viewport's, not the sheet's"
        );
    }

    proptest! {
        /// The frame is the bounds; a point on an edge is at no distance
        /// from it and a point well inside is at some; a corner moved keeps
        /// the opposite corner where it was.
        #[test]
        fn the_frame_bounds_the_viewport_and_its_corners_resize_about_the_opposite_one(
            cx in -1000.0..1000.0f64, cy in -1000.0..1000.0f64,
            w in 1.0..500.0f64, h in 1.0..500.0f64,
            t in 0.0..1.0f64, corner in 0..4usize,
            dx in -200.0..200.0f64, dy in -200.0..200.0f64,
        ) {
            let v = ViewportEntity::new(p(cx, cy), w, h, p(0.0, 0.0), 0.5);
            let frame = v.frame();
            prop_assert!((frame.max().x - frame.min().x - w).abs() < 1e-9);
            prop_assert!((frame.max().y - frame.min().y - h).abs() < 1e-9);
            let on_edge = p(frame.min().x + t * w, frame.min().y);
            prop_assert!(v.distance_to(on_edge) < 1e-9);
            prop_assert!(v.contains(on_edge));
            prop_assert!(v.distance_to(v.center) >= (w.min(h) / 2.0) - 1e-9);
            prop_assert!(!v.contains(p(frame.min().x - 1.0, cy)));
            let corners = v.corners();
            let opposite = corners[(corner + 2) % 4];
            let to = p(corners[corner].x + dx, corners[corner].y + dy);
            if let Some(resized) = v.with_corner(corner, to) {
                let kept = resized.corners().iter().any(|c| c.distance_to(opposite) < 1e-9);
                prop_assert!(kept, "the opposite corner stays: {:?} vs {:?}", resized.corners(), opposite);
                let moved = resized.corners().iter().any(|c| c.distance_to(to) < 1e-9);
                prop_assert!(moved, "the corner lands where it was dragged");
                prop_assert_eq!(resized.scale, v.scale);
                prop_assert_eq!(resized.view_center, v.view_center);
            } else {
                let size = Aabb::from_points(opposite, to);
                let extent = size.max() - size.min();
                prop_assert!(extent.x.abs() < 1e-6 || extent.y.abs() < 1e-6, "only a flat frame is refused");
            }
        }
    }

    #[test]
    fn what_cannot_be_a_viewport_is_refused() {
        assert_eq!(viewport().check(), Ok(()));
        let flat = ViewportEntity {
            width: 0.0,
            ..viewport()
        };
        assert_eq!(flat.check(), Err(ViewportError::Frame));
        let scaleless = ViewportEntity {
            scale: -1.0,
            ..viewport()
        };
        assert_eq!(scaleless.check(), Err(ViewportError::Scale));
        let nowhere = ViewportEntity {
            center: p(f64::NAN, 0.0),
            ..viewport()
        };
        assert_eq!(nowhere.check(), Err(ViewportError::Position));
    }
}
