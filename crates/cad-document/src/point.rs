//! Points (#198): a position, drawn as the drawing's point style — a dot,
//! a cross, an X, a tick, with or without a circle or a square round it,
//! as `$PDMODE` numbers them — at the size `$PDSIZE` gives, in drawing
//! units. The style is a drawing setting: changing it changes every point
//! as one setting changes, and the scene is rebuilt from it.

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Arc, Primitive, Segment, Transform};

/// A point: a place in the drawing, with no size of its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointEntity {
    pub position: Point2,
}

impl PointEntity {
    pub const fn new(position: Point2) -> Self {
        Self { position }
    }

    /// The marker's extent: the style's size round the position, or the
    /// position alone for a dot.
    pub fn bounds(&self, style: PointStyle) -> Aabb {
        let half = style.marker_extent() / 2.0;
        Aabb::from_points(
            self.position - Vec2::new(half, half),
            self.position + Vec2::new(half, half),
        )
    }

    pub fn distance_to(&self, point: Point2) -> f64 {
        self.position.distance_to(point)
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            position: transform.apply(self.position),
        }
    }
}

/// How every point is drawn: the shape `$PDMODE` numbers — 0 a dot, 1
/// nothing, 2 a cross, 3 an X, 4 a tick upwards — with 32 added for a
/// circle round it and 64 for a square — and the size, in drawing units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointStyle {
    pub mode: u8,
    pub size: f64,
}

/// Why a point style is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointStyleError {
    /// Not one of the shapes `$PDMODE` numbers.
    UnknownMode(u8),
    /// The size is not a positive finite number.
    BadSize,
}

impl std::fmt::Display for PointStyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMode(mode) => write!(
                f,
                "{mode} is not a point mode: 0 to 4, with 32 for a circle or 64 for a square"
            ),
            Self::BadSize => write!(f, "the point size must be a positive number"),
        }
    }
}

impl std::error::Error for PointStyleError {}

impl PointStyle {
    pub const DOT: u8 = 0;
    pub const NONE: u8 = 1;
    pub const CROSS: u8 = 2;
    pub const X: u8 = 3;
    pub const TICK: u8 = 4;
    pub const CIRCLE: u8 = 32;
    pub const SQUARE: u8 = 64;
    /// The size of a new drawing's points: one drawing unit.
    pub const DEFAULT_SIZE: f64 = 1.0;

    pub const fn new(mode: u8, size: f64) -> Self {
        Self { mode, size }
    }

    /// The shape without its ring: 0 to 4.
    pub const fn shape(self) -> u8 {
        self.mode & 0b111
    }

    pub const fn has_circle(self) -> bool {
        self.mode & Self::CIRCLE != 0
    }

    pub const fn has_square(self) -> bool {
        self.mode & Self::SQUARE != 0
    }

    /// Whether the style is one the drawing can hold.
    pub fn check(self) -> Result<(), PointStyleError> {
        let ring = self.mode & !(Self::CIRCLE | Self::SQUARE);
        if ring > Self::TICK {
            return Err(PointStyleError::UnknownMode(self.mode));
        }
        if !(self.size.is_finite() && self.size > 0.0) {
            return Err(PointStyleError::BadSize);
        }
        Ok(())
    }

    /// How far the marker reaches across: the size, or nothing for a dot
    /// or a blank without a ring.
    pub fn marker_extent(self) -> f64 {
        if self.has_circle() || self.has_square() || self.shape() >= Self::CROSS {
            self.size
        } else {
            0.0
        }
    }

    /// The marker at `at`, as segments and arcs: what the screen and the
    /// page draw. A dot is a segment of no length, which round caps make a
    /// dot of the line's width.
    pub fn marker(self, at: Point2) -> Vec<Primitive> {
        let half = self.size / 2.0;
        let seg = |a: Vec2, b: Vec2| Primitive::Segment(Segment::new(at + a, at + b));
        let mut out = Vec::new();
        match self.shape() {
            Self::DOT => out.push(seg(Vec2::ZERO, Vec2::ZERO)),
            Self::CROSS => {
                out.push(seg(Vec2::new(-half, 0.0), Vec2::new(half, 0.0)));
                out.push(seg(Vec2::new(0.0, -half), Vec2::new(0.0, half)));
            }
            Self::X => {
                out.push(seg(Vec2::new(-half, -half), Vec2::new(half, half)));
                out.push(seg(Vec2::new(-half, half), Vec2::new(half, -half)));
            }
            Self::TICK => out.push(seg(Vec2::ZERO, Vec2::new(0.0, half))),
            _ => {}
        }
        if self.has_circle() {
            out.push(Primitive::Arc(Arc::new(
                at,
                half,
                0.0,
                std::f64::consts::TAU,
            )));
        }
        if self.has_square() {
            let corners = [
                Vec2::new(-half, -half),
                Vec2::new(half, -half),
                Vec2::new(half, half),
                Vec2::new(-half, half),
            ];
            for i in 0..4 {
                out.push(seg(corners[i], corners[(i + 1) % 4]));
            }
        }
        out
    }
}

impl Default for PointStyle {
    /// A dot, as a new drawing draws points.
    fn default() -> Self {
        Self::new(Self::DOT, Self::DEFAULT_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// #198: the bounds hold the place and every stroke of the marker
        /// at any style; the distance to the point is the distance to its
        /// place; a transform moves the place and the bounds with it.
        #[test]
        fn bounds_hold_the_marker_and_the_hit_is_the_place(
            x in -1000.0..1000.0_f64,
            y in -1000.0..1000.0_f64,
            mode in prop::sample::select(vec![0u8, 1, 2, 3, 4, 32, 35, 64, 66, 96]),
            size in 0.01..50.0_f64,
            px in -1000.0..1000.0_f64,
            py in -1000.0..1000.0_f64,
            dx in -500.0..500.0_f64,
            dy in -500.0..500.0_f64,
        ) {
            let point = PointEntity::new(Point2::new(x, y));
            let style = PointStyle::new(mode, size);
            prop_assert_eq!(style.check(), Ok(()));
            let bounds = point.bounds(style);
            prop_assert!(bounds.contains_point(point.position));
            for primitive in style.marker(point.position) {
                let inside = bounds.expanded(1e-9);
                let ends = match primitive {
                    Primitive::Segment(segment) => vec![segment.start, segment.end],
                    Primitive::Arc(arc) => arc.quadrant_angles().map(|a| arc.point_at(a)).collect(),
                    Primitive::Circle(circle) => vec![circle.point_at(0.0)],
                };
                for end in ends {
                    prop_assert!(inside.contains_point(end), "{end:?} outside {bounds:?}");
                }
            }
            let probe = Point2::new(px, py);
            prop_assert_eq!(point.distance_to(probe), point.position.distance_to(probe));
            let moved = point.transformed(&Transform::translation(Vec2::new(dx, dy)));
            prop_assert!((moved.position.x - (x + dx)).abs() < 1e-9);
            prop_assert!((moved.position.y - (y + dy)).abs() < 1e-9);
            let moved_bounds = moved.bounds(style);
            prop_assert!((moved_bounds.min().x - (bounds.min().x + dx)).abs() < 1e-9);
            prop_assert!((moved_bounds.max().y - (bounds.max().y + dy)).abs() < 1e-9);
        }
    }

    #[test]
    fn modes_are_the_shapes_with_their_rings_and_the_rest_is_refused() {
        for mode in [0, 1, 2, 3, 4, 32, 35, 64, 66, 96, 99] {
            assert_eq!(PointStyle::new(mode, 2.0).check(), Ok(()), "{mode}");
        }
        for mode in [5, 8, 16, 37, 128] {
            assert_eq!(
                PointStyle::new(mode, 2.0).check(),
                Err(PointStyleError::UnknownMode(mode))
            );
        }
        assert_eq!(
            PointStyle::new(0, 0.0).check(),
            Err(PointStyleError::BadSize)
        );
        assert_eq!(
            PointStyle::new(0, f64::NAN).check(),
            Err(PointStyleError::BadSize)
        );
    }

    #[test]
    fn the_marker_is_the_shape_at_the_size_and_a_dot_has_no_extent() {
        let at = Point2::new(10.0, 20.0);
        let dot = PointStyle::default().marker(at);
        assert_eq!(dot, vec![Primitive::Segment(Segment::new(at, at))]);
        assert_eq!(PointStyle::default().marker_extent(), 0.0);
        assert_eq!(PointStyle::new(1, 4.0).marker(at), vec![]);
        let crossed = PointStyle::new(2 + 32 + 64, 4.0);
        let marker = crossed.marker(at);
        assert_eq!(marker.len(), 2 + 1 + 4);
        assert_eq!(crossed.marker_extent(), 4.0);
        let bounds = PointEntity::new(at).bounds(crossed);
        assert_eq!(bounds.min(), Point2::new(8.0, 18.0));
        assert_eq!(bounds.max(), Point2::new(12.0, 22.0));
        assert!(matches!(
            PointStyle::new(3, 4.0).marker(at)[0],
            Primitive::Segment(segment) if segment.start == Point2::new(8.0, 18.0)
        ));
    }
}
