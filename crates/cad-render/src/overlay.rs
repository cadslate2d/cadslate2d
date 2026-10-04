//! The transient overlay frame (DESIGN §12.2).
//!
//! Short-lived interaction geometry: rubber-band previews, selection
//! highlights, the crosshair. It may be rebuilt on every pointer frame and is
//! uploaded to its own small GPU buffers, entirely separately from the
//! committed scene.

use cad_commands::{OverlayItem, SnapMode};
use cad_core::{Aabb, Point2, Vec2, View};
use cad_document::Drawing;
use cad_geometry::Arc;

use crate::{PackedFrame, StyleRole};

/// Half-length of each crosshair arm, in CSS pixels.
const CROSSHAIR_ARM_PIXELS: f64 = 24.0;
/// Half-size of the pick box at the centre of the crosshair, in CSS pixels.
const PICK_BOX_PIXELS: f64 = 3.0;
/// The closest two grid lines may come on screen, in CSS pixels. Below it
/// the grid would turn into a solid fill, so fewer lines are shown.
const MIN_GRID_PIXELS: f64 = 12.0;
/// Every this-many-th grid line is a major line.
const GRID_MAJOR_EVERY: i64 = 5;
/// Half-size of an object-snap marker, in CSS pixels.
const SNAP_MARKER_PIXELS: f64 = 7.0;

/// The glyph of a snap mode as pen strokes, in units of the marker's
/// half-size with +Y up. The centre snap is a circle and has no strokes.
///
/// | Mode | Marker |
/// | --- | --- |
/// | Endpoint | square |
/// | Midpoint | triangle |
/// | Center | circle |
/// | Quadrant | diamond |
/// | Intersection | cross |
/// | Nearest | hourglass |
fn marker_strokes(mode: SnapMode) -> &'static [&'static [(f64, f64)]] {
    const SQUARE: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (1., 1.), (-1., 1.), (-1., -1.)];
    const TRIANGLE: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (0., 1.), (-1., -1.)];
    const DIAMOND: &[(f64, f64)] = &[(0., -1.), (1., 0.), (0., 1.), (-1., 0.), (0., -1.)];
    const HOURGLASS: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (-1., 1.), (1., 1.), (-1., -1.)];
    const RISING: &[(f64, f64)] = &[(-1., -1.), (1., 1.)];
    const FALLING: &[(f64, f64)] = &[(-1., 1.), (1., -1.)];
    match mode {
        SnapMode::Endpoint => &[SQUARE],
        SnapMode::Midpoint => &[TRIANGLE],
        SnapMode::Center => &[],
        SnapMode::Quadrant => &[DIAMOND],
        SnapMode::Intersection => &[RISING, FALLING],
        SnapMode::Nearest => &[HOURGLASS],
    }
}

/// The spacing the grid is drawn at: `spacing` times the smallest of 1, 2,
/// 5, 10, 20, 50, … that keeps lines at least [`MIN_GRID_PIXELS`] apart at
/// `scale` pixels per unit. Lines therefore always sit on multiples of the
/// setting. `None` when there is nothing sensible to draw.
pub fn displayed_grid_spacing(spacing: f64, scale: f64) -> Option<f64> {
    let usable = spacing.is_finite() && spacing > 0.0 && scale.is_finite() && scale > 0.0;
    if !usable {
        return None;
    }
    (0..=308)
        .flat_map(|decade| [1.0, 2.0, 5.0].map(|step| spacing * step * 10f64.powi(decade)))
        .take_while(|shown| shown.is_finite())
        .find(|shown| shown * scale >= MIN_GRID_PIXELS)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverlayFrame {
    frame: PackedFrame,
}

impl OverlayFrame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn frame(&self) -> &PackedFrame {
        &self.frame
    }

    /// Rebuilds the frame from the session's semantic overlay description.
    ///
    /// Unlike the committed scene, the overlay *does* depend on the view:
    /// screen-space decorations such as the crosshair are sized here.
    pub fn rebuild(&mut self, items: &[OverlayItem], drawing: &Drawing, view: &View) {
        // The overlay is rebuilt freely, so it can always sit on the view centre.
        self.frame.reset(view.center());
        for item in items {
            match item {
                OverlayItem::Grid { spacing } => self.push_grid(*spacing, view),
                OverlayItem::Highlight(id) => {
                    if let Some(entity) = drawing.entity(*id) {
                        self.frame.push_entity(StyleRole::Selection, entity);
                    }
                }
                OverlayItem::PreviewSegment { start, end } => {
                    self.frame.push_segment(StyleRole::Preview, *start, *end);
                }
                OverlayItem::PreviewEntity(entity) => {
                    self.frame.push_entity(StyleRole::Preview, entity)
                }
                OverlayItem::PreviewTranslated { ids, offset } => {
                    for entity in ids.iter().filter_map(|id| drawing.entity(*id)) {
                        self.frame
                            .push_entity(StyleRole::Preview, &entity.translated(*offset));
                    }
                }
                OverlayItem::SelectionBox { from, to, crossing } => {
                    let style = if *crossing {
                        StyleRole::SelectionCrossing
                    } else {
                        StyleRole::SelectionWindow
                    };
                    let corners = Aabb::from_points(*from, *to).corners();
                    for (index, corner) in corners.iter().enumerate() {
                        self.frame.push_segment(
                            style,
                            *corner,
                            corners[(index + 1) % corners.len()],
                        );
                    }
                }
                OverlayItem::TrackingLine { from, through } => {
                    self.push_tracking_line(*from, *through, view)
                }
                OverlayItem::SnapMarker { mode, at } => self.push_snap_marker(*mode, *at, view),
                OverlayItem::Crosshair { at } => self.push_crosshair(*at, view),
            }
        }
    }

    /// Grid lines across the visible area, on multiples of the displayed
    /// spacing in world coordinates — so the grid stays put under pan and zoom.
    /// Minor lines first, then major lines over them.
    fn push_grid(&mut self, spacing: f64, view: &View) {
        let Some(shown) = displayed_grid_spacing(spacing, view.scale()) else {
            return;
        };
        let visible = view.visible_bounds();
        let (min, max) = (visible.min(), visible.max());
        // Line numbers: line `n` is at `n × shown`.
        let columns = (min.x / shown).ceil() as i64..=(max.x / shown).floor() as i64;
        let rows = (min.y / shown).ceil() as i64..=(max.y / shown).floor() as i64;

        for (style, major) in [(StyleRole::GridMinor, false), (StyleRole::GridMajor, true)] {
            let wanted = |line: &i64| (line % GRID_MAJOR_EVERY == 0) == major;
            for column in columns.clone().filter(wanted) {
                let x = column as f64 * shown;
                self.frame
                    .push_segment(style, Point2::new(x, min.y), Point2::new(x, max.y));
            }
            for row in rows.clone().filter(wanted) {
                let y = row as f64 * shown;
                self.frame
                    .push_segment(style, Point2::new(min.x, y), Point2::new(max.x, y));
            }
        }
    }

    /// A ray from `from` through `through`, long enough to leave the view
    /// from wherever it starts.
    fn push_tracking_line(&mut self, from: Point2, through: Point2, view: &View) {
        let direction = through - from;
        let length = direction.length();
        if length > 0.0 {
            let visible = view.visible_bounds();
            let reach = from.distance_to(view.center()) + visible.min().distance_to(visible.max());
            let end = from + direction * (reach / length);
            self.frame.push_segment(StyleRole::Tracking, from, end);
        }
    }

    /// The marker is sized in pixels here, so it looks the same at any zoom.
    fn push_snap_marker(&mut self, mode: SnapMode, at: Point2, view: &View) {
        let half = view.pixels_to_world(SNAP_MARKER_PIXELS);
        if mode == SnapMode::Center {
            let circle = Arc::new(at, half, 0.0, std::f64::consts::TAU);
            self.frame.push_arc(StyleRole::SnapMarker, &circle);
        }
        for stroke in marker_strokes(mode) {
            for pair in stroke.windows(2) {
                let corner = |(x, y): (f64, f64)| at + Vec2::new(x * half, y * half);
                self.frame
                    .push_segment(StyleRole::SnapMarker, corner(pair[0]), corner(pair[1]));
            }
        }
    }

    fn push_crosshair(&mut self, at: Point2, view: &View) {
        let arm = view.pixels_to_world(CROSSHAIR_ARM_PIXELS);
        let half = view.pixels_to_world(PICK_BOX_PIXELS);
        let mut segment = |from: Vec2, to: Vec2| {
            self.frame
                .push_segment(StyleRole::Crosshair, at + from, at + to);
        };

        segment(Vec2::new(-arm, 0.0), Vec2::new(arm, 0.0));
        segment(Vec2::new(0.0, -arm), Vec2::new(0.0, arm));

        let corners = [
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ];
        for (index, corner) in corners.iter().enumerate() {
            segment(*corner, corners[(index + 1) % corners.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use cad_commands::testing::Harness;
    use cad_core::ScreenPoint;

    use super::*;
    use crate::{DrawBatch, SceneCache};

    fn overlay_of(h: &Harness) -> OverlayFrame {
        let mut overlay = OverlayFrame::new();
        overlay.rebuild(&h.session.overlay(), h.session.drawing(), h.session.view());
        overlay
    }

    fn styles(overlay: &OverlayFrame) -> Vec<StyleRole> {
        overlay
            .frame()
            .batches()
            .iter()
            .map(|batch| batch.style)
            .collect()
    }

    #[test]
    fn nothing_to_show_yields_an_empty_frame() {
        let overlay = overlay_of(&Harness::new());
        assert_eq!(overlay.frame().segment_count(), 0);
        assert!(overlay.frame().batches().is_empty());
    }

    #[test]
    fn the_crosshair_is_two_arms_and_a_pick_box_sized_in_pixels() {
        let mut h = Harness::new();
        h.move_to(400.0, 300.0);

        let overlay = overlay_of(&h);
        assert_eq!(
            overlay.frame().batches(),
            vec![DrawBatch {
                style: StyleRole::Crosshair,
                first: 0,
                count: 6
            }]
        );
        // The horizontal arm, relative to the view centre (which is the cursor here).
        assert_eq!(&overlay.frame().segments()[..4], &[-24.0, 0.0, 24.0, 0.0]);

        // Zoomed in 4×, the same pixel size is a quarter of the world size.
        h.session
            .wheel(ScreenPoint::new(400.0, 300.0), -500.0, Default::default());
        let scale = h.session.view().scale() as f32;
        let zoomed = overlay_of(&h);
        assert!((zoomed.frame().segments()[2] - 24.0 / scale).abs() < 1e-4);
    }

    #[test]
    fn the_rubber_band_is_a_preview_segment_under_the_crosshair() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        h.move_to(450.0, 300.0);

        let overlay = overlay_of(&h);
        assert_eq!(
            styles(&overlay),
            vec![StyleRole::Preview, StyleRole::Crosshair]
        );
        // From the previous point (0,0) to the cursor (50,0), relative to the view centre (0,0).
        assert_eq!(&overlay.frame().segments()[..4], &[0.0, 0.0, 50.0, 0.0]);
    }

    #[test]
    fn the_same_transient_state_always_yields_the_same_frame() {
        let build = || {
            let mut h = Harness::new();
            h.enter_all(&["L", "10,10"]);
            h.move_to(123.0, 456.0);
            overlay_of(&h)
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn cancelling_a_command_leaves_no_preview_behind() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        h.move_to(450.0, 300.0);
        h.escape();
        assert_eq!(styles(&overlay_of(&h)), vec![StyleRole::Crosshair]);
    }

    #[test]
    fn rebuilding_reuses_the_frame_buffers() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        h.move_to(100.0, 100.0);
        let mut overlay = overlay_of(&h);
        let capacity = overlay.frame.segments().as_ptr();

        for step in 0..100 {
            h.move_to(f64::from(step), 200.0);
            overlay.rebuild(&h.session.overlay(), h.session.drawing(), h.session.view());
        }
        assert_eq!(
            overlay.frame.segments().as_ptr(),
            capacity,
            "segment buffer was reallocated"
        );
    }

    #[test]
    fn selected_entities_are_drawn_as_highlights_under_the_crosshair() {
        let mut h = Harness::new();
        h.enter_all(&["L", "-100,0", "100,0", ""]);
        h.click(400.0, 300.0);

        let overlay = overlay_of(&h);
        assert_eq!(
            styles(&overlay),
            vec![StyleRole::Selection, StyleRole::Crosshair]
        );
        assert_eq!(&overlay.frame().segments()[..4], &[-100.0, 0.0, 100.0, 0.0]);
    }

    #[test]
    fn selecting_never_changes_the_committed_scene() {
        let mut h = Harness::new();
        h.enter_all(&["L", "-100,0", "100,0", ""]);
        let mut scenes = SceneCache::new();
        let revision = scenes.scene(h.session.drawing()).revision();

        h.click(400.0, 300.0);
        h.escape();
        h.click(400.0, 300.0);

        assert_eq!(scenes.scene(h.session.drawing()).revision(), revision);
        assert_eq!(scenes.build_count(), 1);
    }

    #[test]
    fn the_highlight_follows_undo_and_redo() {
        let mut h = Harness::new();
        h.enter_all(&["L", "-100,0", "100,0", ""]);
        h.click(400.0, 300.0);
        assert!(styles(&overlay_of(&h)).contains(&StyleRole::Selection));

        h.undo();
        assert!(
            !styles(&overlay_of(&h)).contains(&StyleRole::Selection),
            "no highlight for a line that is gone"
        );
    }

    #[test]
    fn a_highlight_for_a_missing_entity_is_skipped() {
        let h = Harness::new();
        let mut overlay = OverlayFrame::new();
        let ghost = OverlayItem::Highlight(cad_core::EntityId::from_raw(999));
        overlay.rebuild(&[ghost], h.session.drawing(), h.session.view());
        assert_eq!(overlay.frame().segment_count(), 0);
    }

    // ── snap markers ──

    /// The overlay of a bare marker at the origin of a 1px-per-unit view.
    fn marker_frame(mode: SnapMode, view: &View) -> OverlayFrame {
        let mut overlay = OverlayFrame::new();
        let at = Point2::new(10.0, 20.0);
        overlay.rebuild(
            &[OverlayItem::SnapMarker { mode, at }],
            &Drawing::new(),
            view,
        );
        overlay
    }

    fn view() -> View {
        View::new(800.0, 600.0)
    }

    #[test]
    fn every_snap_mode_has_its_own_marker() {
        let view = view();
        let frames: Vec<OverlayFrame> = SnapMode::ALL
            .iter()
            .map(|mode| marker_frame(*mode, &view))
            .collect();
        for (index, frame) in frames.iter().enumerate() {
            let drawn = frame.frame().segment_count() + frame.frame().arc_count();
            assert!(drawn > 0, "{:?} draws nothing", SnapMode::ALL[index]);
            for other in &frames[index + 1..] {
                assert_ne!(frame, other, "two modes share a marker");
            }
        }
    }

    #[test]
    fn markers_are_drawn_in_the_snap_marker_style_only() {
        let view = view();
        for mode in SnapMode::ALL {
            let overlay = marker_frame(mode, &view);
            let batches = [overlay.frame().batches(), overlay.frame().arc_batches()].concat();
            assert!(
                batches
                    .iter()
                    .all(|batch| batch.style == StyleRole::SnapMarker),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn the_centre_marker_is_a_circle_and_the_others_are_strokes() {
        let view = view();
        let centre = marker_frame(SnapMode::Center, &view);
        assert_eq!(
            (centre.frame().arc_count(), centre.frame().segment_count()),
            (1, 0)
        );
        let counts: Vec<u32> = [
            SnapMode::Endpoint,
            SnapMode::Midpoint,
            SnapMode::Quadrant,
            SnapMode::Intersection,
            SnapMode::Nearest,
        ]
        .iter()
        .map(|mode| marker_frame(*mode, &view).frame().segment_count())
        .collect();
        assert_eq!(counts, vec![4, 3, 4, 2, 4]);
    }

    /// Markers are a fixed size on screen: twice the zoom, half the world size.
    #[test]
    fn markers_keep_their_pixel_size_at_any_zoom() {
        let extent_in_pixels = |view: &View| {
            let overlay = marker_frame(SnapMode::Endpoint, view);
            let xs: Vec<f32> = overlay
                .frame()
                .segments()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|point| point[0])
                .collect();
            let width = xs.iter().copied().fold(f32::MIN, f32::max)
                - xs.iter().copied().fold(f32::MAX, f32::min);
            f64::from(width) * view.scale()
        };
        let mut zoomed = view();
        zoomed.zoom_about(ScreenPoint::new(400.0, 300.0), 37.5);
        for view in [view(), zoomed] {
            let pixels = extent_in_pixels(&view);
            assert!(
                (pixels - 2.0 * SNAP_MARKER_PIXELS).abs() < 1e-3,
                "{pixels}px wide"
            );
        }
    }

    #[test]
    fn hovering_a_snap_point_draws_the_marker_under_the_crosshair() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "100,0", "", "L"]);
        h.move_to_world(Point2::new(99.0, 1.0));
        assert_eq!(
            styles(&overlay_of(&h)),
            vec![StyleRole::SnapMarker, StyleRole::Crosshair]
        );
    }

    // ── grid ──

    fn grid_frame(spacing: f64, view: &View) -> OverlayFrame {
        let mut overlay = OverlayFrame::new();
        overlay.rebuild(&[OverlayItem::Grid { spacing }], &Drawing::new(), view);
        overlay
    }

    /// World coordinates of the vertical and horizontal lines of one style.
    fn grid_lines(overlay: &OverlayFrame, style: StyleRole) -> (Vec<f64>, Vec<f64>) {
        let frame = overlay.frame();
        let origin = frame.origin();
        let (mut columns, mut rows) = (Vec::new(), Vec::new());
        for batch in frame.batches().iter().filter(|batch| batch.style == style) {
            let (first, count) = (batch.first as usize, batch.count as usize);
            for segment in &frame.segments().as_chunks::<4>().0[first..first + count] {
                let [x0, y0, x1, y1] = segment.map(f64::from);
                if (x0 - x1).abs() < f64::EPSILON {
                    columns.push((origin.x + x0).round());
                } else {
                    assert!((y0 - y1).abs() < f64::EPSILON, "a grid line is slanted");
                    rows.push((origin.y + y0).round());
                }
            }
        }
        (columns, rows)
    }

    #[test]
    fn the_displayed_spacing_steps_1_2_5_so_lines_never_crowd() {
        // At one pixel per unit, a spacing of 10 is 10px: too close; 20 is shown.
        assert_eq!(displayed_grid_spacing(10.0, 1.0), Some(20.0));
        assert_eq!(displayed_grid_spacing(10.0, 2.0), Some(10.0));
        assert_eq!(
            displayed_grid_spacing(10.0, 100.0),
            Some(10.0),
            "never finer than the setting"
        );
        assert_eq!(displayed_grid_spacing(10.0, 0.3), Some(50.0));
        assert_eq!(displayed_grid_spacing(10.0, 0.1), Some(200.0));
        assert_eq!(displayed_grid_spacing(10.0, 0.001), Some(20_000.0));
        assert_eq!(displayed_grid_spacing(2.5, 1.0), Some(12.5));
        for scale in [1e-9, 1e-3, 0.07, 1.0, 13.0, 1e6] {
            let shown = displayed_grid_spacing(10.0, scale).unwrap();
            assert!(shown * scale >= MIN_GRID_PIXELS, "crowded at scale {scale}");
        }
    }

    #[test]
    fn a_nonsensical_grid_is_not_drawn() {
        for (spacing, scale) in [
            (0.0, 1.0),
            (-10.0, 1.0),
            (f64::NAN, 1.0),
            (10.0, 0.0),
            (f64::INFINITY, 1.0),
        ] {
            assert_eq!(displayed_grid_spacing(spacing, scale), None);
        }
        assert_eq!(grid_frame(0.0, &view()).frame().segment_count(), 0);
    }

    #[test]
    fn grid_lines_cover_the_view_with_every_fifth_one_major() {
        // 800×600 at 1px per unit around the origin; spacing 20 is shown as is.
        let overlay = grid_frame(20.0, &view());
        let (minor_columns, minor_rows) = grid_lines(&overlay, StyleRole::GridMinor);
        let (major_columns, major_rows) = grid_lines(&overlay, StyleRole::GridMajor);

        assert_eq!(
            major_columns,
            vec![
                -400.0, -300.0, -200.0, -100.0, 0.0, 100.0, 200.0, 300.0, 400.0
            ]
        );
        assert_eq!(
            major_rows,
            vec![-300.0, -200.0, -100.0, 0.0, 100.0, 200.0, 300.0]
        );
        assert_eq!(minor_columns.len(), 41 - 9);
        assert_eq!(minor_rows.len(), 31 - 7);
        assert!(
            minor_columns
                .iter()
                .all(|x| x % 20.0 == 0.0 && x % 100.0 != 0.0)
        );
        // Minor lines are drawn first, so major lines lie over them.
        assert_eq!(
            styles(&overlay),
            vec![StyleRole::GridMinor, StyleRole::GridMajor]
        );
    }

    /// #82: the grid stays aligned with world coordinates through pan and zoom.
    #[test]
    fn grid_lines_sit_on_world_multiples_of_the_spacing_wherever_the_view_is() {
        let mut view = view();
        let mut seen = 0;
        for step in 0..12 {
            view.pan_by_pixels(137.0 * f64::from(step), -59.0 * f64::from(step));
            view.zoom_about(
                ScreenPoint::new(123.0, 456.0),
                if step % 2 == 0 { 1.7 } else { 0.4 },
            );
            let shown = displayed_grid_spacing(10.0, view.scale()).unwrap();
            let overlay = grid_frame(10.0, &view);
            let visible = view.visible_bounds();
            for style in [StyleRole::GridMinor, StyleRole::GridMajor] {
                let frame = overlay.frame();
                let origin = frame.origin();
                for batch in frame.batches().iter().filter(|batch| batch.style == style) {
                    let (first, count) = (batch.first as usize, batch.count as usize);
                    for segment in &frame.segments().as_chunks::<4>().0[first..first + count] {
                        let [x0, y0, x1, _] = segment.map(f64::from);
                        let vertical = (x0 - x1).abs() < shown * 1e-6;
                        let at = if vertical {
                            origin.x + x0
                        } else {
                            origin.y + y0
                        };
                        let line = at / shown;
                        // On a multiple of the spacing, to f32 precision.
                        assert!(
                            (line - line.round()).abs() < 1e-3,
                            "line at {at}, spacing {shown}"
                        );
                        let major = (line.round() as i64) % GRID_MAJOR_EVERY == 0;
                        assert_eq!(major, style == StyleRole::GridMajor);
                        let (low, high) = if vertical {
                            (visible.min().x, visible.max().x)
                        } else {
                            (visible.min().y, visible.max().y)
                        };
                        assert!(at >= low - shown * 1e-3 && at <= high + shown * 1e-3);
                        seen += 1;
                    }
                }
            }
        }
        assert!(seen > 500);
    }

    #[test]
    fn the_number_of_grid_lines_is_bounded_at_any_zoom() {
        let mut view = View::new(3840.0, 2160.0);
        for _ in 0..40 {
            view.zoom_about(ScreenPoint::new(1900.0, 1000.0), 0.5);
            let count = grid_frame(10.0, &view).frame().segment_count();
            assert!(
                count <= (3840 + 2160) / MIN_GRID_PIXELS as u32 + 4,
                "{count} lines"
            );
            assert!(count > 100);
        }
    }

    #[test]
    fn the_grid_lies_under_everything_else_in_the_overlay() {
        let mut h = Harness::new();
        h.toggle(cad_commands::Toggle::Grid);
        h.enter_all(&["L", "0,0", "100,0", ""]);
        h.click_world(Point2::new(50.0, 0.0));
        h.move_to(10.0, 10.0);
        assert_eq!(
            styles(&overlay_of(&h)),
            vec![
                StyleRole::GridMinor,
                StyleRole::GridMajor,
                StyleRole::Selection,
                StyleRole::Crosshair
            ]
        );
    }

    // ── polar tracking ──

    #[test]
    fn a_tracking_line_runs_from_its_base_past_the_edge_of_the_view() {
        let view = view();
        let mut overlay = OverlayFrame::new();
        let item = OverlayItem::TrackingLine {
            from: Point2::new(-350.0, -250.0),
            through: Point2::new(-340.0, -240.0),
        };
        overlay.rebuild(&[item], &Drawing::new(), &view);

        assert_eq!(styles(&overlay), vec![StyleRole::Tracking]);
        let ends = overlay.frame().segments();
        let origin = overlay.frame().origin();
        let start = origin + Vec2::new(f64::from(ends[0]), f64::from(ends[1]));
        let end = origin + Vec2::new(f64::from(ends[2]), f64::from(ends[3]));
        assert_eq!(start, Point2::new(-350.0, -250.0));
        // Along the 45° direction, and beyond the far corner of an 800×600 view.
        assert!((end.x - start.x - (end.y - start.y)).abs() < 1e-3);
        assert!(!view.visible_bounds().contains_point(end));
    }

    #[test]
    fn a_tracking_line_without_a_direction_is_skipped() {
        let mut overlay = OverlayFrame::new();
        let at = Point2::new(5.0, 5.0);
        let item = OverlayItem::TrackingLine {
            from: at,
            through: at,
        };
        overlay.rebuild(&[item], &Drawing::new(), &view());
        assert_eq!(overlay.frame().segment_count(), 0);
    }

    #[test]
    fn polar_tracking_draws_under_the_rubber_band() {
        let mut h = Harness::new();
        h.toggle(cad_commands::Toggle::Polar);
        h.enter_all(&["L", "0,0"]);
        h.move_to_world(Point2::new(90.0, 4.0));
        assert_eq!(
            styles(&overlay_of(&h)),
            vec![
                StyleRole::Tracking,
                StyleRole::Preview,
                StyleRole::Crosshair
            ]
        );
    }

    #[test]
    fn a_selection_box_is_four_segments_in_the_style_of_its_direction() {
        let mut h = Harness::new();
        h.click(300.0, 200.0); // empty space: starts a box
        h.move_to(500.0, 400.0); // to the right: a window
        let window = overlay_of(&h);
        assert_eq!(
            styles(&window),
            vec![StyleRole::SelectionWindow, StyleRole::Crosshair]
        );
        assert_eq!(window.frame().batches()[0].count, 4);

        h.move_to(100.0, 400.0); // to the left: a crossing box
        assert_eq!(
            styles(&overlay_of(&h)),
            vec![StyleRole::SelectionCrossing, StyleRole::Crosshair]
        );
    }

    #[test]
    fn a_move_preview_draws_the_sources_at_their_new_position_without_touching_the_scene() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "100,0", ""]);
        h.click(450.0, 300.0);
        h.enter_all(&["M", "0,0"]);
        h.move_to(400.0, 250.0); // 50 units up

        let mut scenes = SceneCache::new();
        let revision = scenes.scene(h.session.drawing()).revision();
        let overlay = overlay_of(&h);
        let batches = overlay.frame().batches();
        let preview = batches
            .iter()
            .find(|batch| batch.style == StyleRole::Preview)
            .unwrap();
        // The rubber band from the base point, then the line 50 units higher.
        assert_eq!(preview.count, 2);
        let moved = &overlay.frame().segments()[(preview.first as usize + 1) * 4..][..4];
        assert_eq!(moved, &[0.0, 50.0, 100.0, 50.0]);
        assert_eq!(scenes.scene(h.session.drawing()).revision(), revision);
    }

    /// DESIGN §26.3 criteria 5 and 6: the rubber band changes only overlay
    /// data; the committed scene is neither rebuilt nor given a new revision.
    #[test]
    fn pointer_movement_changes_the_overlay_but_never_the_committed_scene() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "100,0"]);
        let mut scenes = SceneCache::new();
        let revision = scenes.scene(h.session.drawing()).revision();
        let builds = scenes.build_count();

        let mut previous = overlay_of(&h);
        for step in 1..=200 {
            h.move_to(400.0 + f64::from(step), 300.0 - f64::from(step));
            let overlay = overlay_of(&h);
            assert_ne!(overlay, previous, "the overlay must follow the pointer");
            previous = overlay;
            assert_eq!(scenes.scene(h.session.drawing()).revision(), revision);
        }
        assert_eq!(scenes.build_count(), builds);
    }
}
