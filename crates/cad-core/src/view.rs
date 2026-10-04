//! The view: the one mapping between screen and world (ADR-0003).
//!
//! Picking, snapping, zoom-about-cursor, and rendering all go through this
//! type so they can never disagree. The view is session state: it is not part
//! of the drawing, not undoable, and not saved.
//!
//! Screen coordinates are CSS pixels with the origin at the top-left of the
//! canvas and Y pointing down. World Y points up.

use crate::{Aabb, Point2, Vec2};

/// A position on the canvas, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

impl ScreenPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A display/picking tolerance in CSS pixels (DESIGN §18.3). It is converted
/// to world units through the view at the moment of use, so it stays constant
/// on screen at every zoom level. Never use it for geometric equality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelTolerance(pub f64);

/// Maps positions relative to a render origin into clip space:
/// `clip = position * scale + translate`.
///
/// Computed in `f64` and narrowed once, so the large-magnitude subtraction of
/// origin and view centre never happens in single precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipTransform {
    pub scale: [f32; 2],
    pub translate: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct View {
    center: Point2,
    /// CSS pixels per world unit.
    scale: f64,
    width: f64,
    height: f64,
}

impl View {
    pub const MIN_SCALE: f64 = 1e-6;
    pub const MAX_SCALE: f64 = 1e9;

    /// A view of the given canvas size, centred on the world origin at one
    /// pixel per world unit.
    pub fn new(width: f64, height: f64) -> Self {
        let mut view = Self {
            center: Point2::ORIGIN,
            scale: 1.0,
            width: 1.0,
            height: 1.0,
        };
        view.resize(width, height);
        view
    }

    pub fn center(&self) -> Point2 {
        self.center
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    /// Changes the canvas size, keeping the same world point at the centre.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
    }

    pub fn world_to_screen(&self, point: Point2) -> ScreenPoint {
        let offset = point - self.center;
        ScreenPoint::new(
            self.width / 2.0 + offset.x * self.scale,
            self.height / 2.0 - offset.y * self.scale,
        )
    }

    pub fn screen_to_world(&self, point: ScreenPoint) -> Point2 {
        self.center + self.offset_from_center(point)
    }

    /// Moves the drawing by a screen-space drag: content follows the pointer.
    pub fn pan_by_pixels(&mut self, dx: f64, dy: f64) {
        self.center = self.center - Vec2::new(dx / self.scale, -dy / self.scale);
    }

    /// Multiplies the scale by `factor`, keeping the world point under
    /// `anchor` fixed on screen. The scale is clamped to a sane range.
    pub fn zoom_about(&mut self, anchor: ScreenPoint, factor: f64) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let fixed = self.screen_to_world(anchor);
        self.scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        self.center = fixed - self.offset_from_center(anchor);
    }

    /// The world-space length of `pixels` on screen.
    pub fn pixels_to_world(&self, pixels: f64) -> f64 {
        pixels / self.scale
    }

    /// The world-space box covered by a pick of `tolerance` around `at`.
    pub fn pick_bounds(&self, at: ScreenPoint, tolerance: PixelTolerance) -> Aabb {
        let center = self.screen_to_world(at);
        Aabb::from_points(center, center).expanded(self.pixels_to_world(tolerance.0))
    }

    /// The world-space box currently visible on the canvas.
    pub fn visible_bounds(&self) -> Aabb {
        Aabb::from_points(
            self.screen_to_world(ScreenPoint::new(0.0, self.height)),
            self.screen_to_world(ScreenPoint::new(self.width, 0.0)),
        )
    }

    /// The transform that places geometry stored relative to `origin`.
    pub fn clip_transform(&self, origin: Point2) -> ClipTransform {
        let sx = 2.0 * self.scale / self.width;
        let sy = 2.0 * self.scale / self.height;
        let offset = origin - self.center;
        ClipTransform {
            scale: [sx as f32, sy as f32],
            translate: [(offset.x * sx) as f32, (offset.y * sy) as f32],
        }
    }

    /// World-space offset of a screen point from the view centre.
    fn offset_from_center(&self, point: ScreenPoint) -> Vec2 {
        Vec2::new(
            (point.x - self.width / 2.0) / self.scale,
            -(point.y - self.height / 2.0) / self.scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn the_view_centre_maps_to_the_canvas_centre() {
        let view = View::new(800.0, 600.0);
        assert_eq!(
            view.world_to_screen(Point2::ORIGIN),
            ScreenPoint::new(400.0, 300.0)
        );
        assert_eq!(
            view.screen_to_world(ScreenPoint::new(400.0, 300.0)),
            Point2::ORIGIN
        );
    }

    #[test]
    fn world_y_points_up_and_screen_y_points_down() {
        let view = View::new(800.0, 600.0);
        assert_eq!(
            view.world_to_screen(Point2::new(10.0, 20.0)),
            ScreenPoint::new(410.0, 280.0)
        );
    }

    #[test]
    fn panning_moves_content_with_the_pointer() {
        let mut view = View::new(800.0, 600.0);
        let before = view.world_to_screen(Point2::new(5.0, 5.0));
        view.pan_by_pixels(30.0, -12.0);
        let after = view.world_to_screen(Point2::new(5.0, 5.0));
        assert_eq!(after, ScreenPoint::new(before.x + 30.0, before.y - 12.0));
    }

    #[test]
    fn zoom_scales_and_is_clamped() {
        let mut view = View::new(800.0, 600.0);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), 2.0);
        assert_eq!(view.scale(), 2.0);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), 1e30);
        assert_eq!(view.scale(), View::MAX_SCALE);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), 1e-30);
        assert_eq!(view.scale(), View::MIN_SCALE);
    }

    #[test]
    fn invalid_zoom_factors_are_ignored() {
        let mut view = View::new(800.0, 600.0);
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            view.zoom_about(ScreenPoint::new(10.0, 10.0), factor);
        }
        assert_eq!(view, View::new(800.0, 600.0));
    }

    #[test]
    fn resize_keeps_the_centre_and_never_collapses() {
        let mut view = View::new(800.0, 600.0);
        view.pan_by_pixels(100.0, 50.0);
        let center = view.center();
        view.resize(0.0, -5.0);
        assert_eq!(view.center(), center);
        assert_eq!((view.width(), view.height()), (1.0, 1.0));
    }

    #[test]
    fn pick_bounds_are_constant_in_pixels_at_any_zoom() {
        let mut view = View::new(800.0, 600.0);
        let at = ScreenPoint::new(100.0, 100.0);
        for factor in [1.0, 8.0, 0.01] {
            view.zoom_about(at, factor);
            let bounds = view.pick_bounds(at, PixelTolerance(5.0));
            let min = view.world_to_screen(bounds.min());
            let max = view.world_to_screen(bounds.max());
            assert!(
                close(max.x - min.x, 10.0, 1e-6),
                "width at scale {}",
                view.scale()
            );
            assert!(
                close(min.y - max.y, 10.0, 1e-6),
                "height at scale {}",
                view.scale()
            );
        }
    }

    #[test]
    fn visible_bounds_match_the_canvas_corners() {
        let mut view = View::new(800.0, 600.0);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), 2.0);
        let bounds = view.visible_bounds();
        assert_eq!(bounds.min(), Point2::new(-200.0, -150.0));
        assert_eq!(bounds.max(), Point2::new(200.0, 150.0));
    }

    #[test]
    fn clip_transform_maps_the_canvas_to_the_unit_square() {
        let mut view = View::new(800.0, 600.0);
        view.pan_by_pixels(-40.0, 0.0); // centre is now at world x = 40
        let origin = Point2::new(1000.0, 0.0);
        let t = view.clip_transform(origin);
        let clip = |world: Point2| {
            let rel = world - origin;
            (
                rel.x as f32 * t.scale[0] + t.translate[0],
                rel.y as f32 * t.scale[1] + t.translate[1],
            )
        };
        let (cx, cy) = clip(view.center());
        assert!(cx.abs() < 1e-6 && cy.abs() < 1e-6);
        let (rx, ty) = clip(view.visible_bounds().max());
        assert!((rx - 1.0).abs() < 1e-6 && (ty - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clip_transform_stays_precise_far_from_the_world_origin() {
        // Survey-style coordinates: f32 alone could not resolve 0.001 units here.
        let far = Point2::new(5_000_000.0, 7_000_000.0);
        let mut view = View::new(800.0, 600.0);
        view.pan_by_pixels(-far.x, far.y);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), 1000.0);

        let origin = Point2::new(far.x + 0.25, far.y - 0.125);
        let t = view.clip_transform(origin);
        let expected_x = (origin.x - view.center().x) * 2.0 * view.scale() / view.width();
        assert!(close(f64::from(t.translate[0]), expected_x, 1e-5));
    }

    fn any_view() -> impl Strategy<Value = View> {
        (
            100.0..4000.0_f64,
            100.0..4000.0_f64,
            -1e5..1e5_f64,
            -1e5..1e5_f64,
            -6.0..6.0_f64,
        )
            .prop_map(|(w, h, dx, dy, zoom_exp)| {
                let mut view = View::new(w, h);
                view.pan_by_pixels(dx, dy);
                view.zoom_about(
                    ScreenPoint::new(w / 2.0, h / 2.0),
                    10f64.powf(zoom_exp / 2.0),
                );
                view
            })
    }

    proptest! {
        #[test]
        fn screen_world_round_trip(view in any_view(), x in 0.0..4000.0_f64, y in 0.0..4000.0_f64) {
            let screen = ScreenPoint::new(x, y);
            let back = view.world_to_screen(view.screen_to_world(screen));
            prop_assert!(close(back.x, x, 1e-6) && close(back.y, y, 1e-6), "{back:?}");
        }

        #[test]
        fn zoom_keeps_the_world_point_under_the_cursor(
            mut view in any_view(),
            x in 0.0..4000.0_f64,
            y in 0.0..4000.0_f64,
            factor in 0.1..10.0_f64,
        ) {
            let anchor = ScreenPoint::new(x, y);
            let before = view.screen_to_world(anchor);
            view.zoom_about(anchor, factor);
            let after = view.world_to_screen(before);
            prop_assert!(close(after.x, x, 1e-6) && close(after.y, y, 1e-6), "{after:?}");
        }
    }
}
