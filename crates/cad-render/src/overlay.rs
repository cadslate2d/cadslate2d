//! The transient overlay frame (DESIGN §12.2).
//!
//! Short-lived interaction geometry: rubber-band previews, selection
//! highlights, the crosshair. It may be rebuilt on every pointer frame and is
//! uploaded to its own small GPU buffers, entirely separately from the
//! committed scene.

use std::f64::consts::{PI, TAU};

use cad_commands::constraint_display::{Glyph, glyph_square};
use cad_commands::{OverlayItem, SnapMode};
use cad_core::{Aabb, Point2, Vec2, View};
use cad_document::{Drawing, Entity, LayoutId};
use cad_geometry::{Arc, Primitive};

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
/// Half-size of a grip square, in CSS pixels.
const GRIP_PIXELS: f64 = 4.0;
/// Half-size of the hovered or dragged grip: a little larger.
const HOT_GRIP_PIXELS: f64 = 5.0;
/// Half-size of an object-snap marker, in CSS pixels.
/// Half the size of an acquired point's cross, in pixels (#220).
const ACQUIRED_MARK_PIXELS: f64 = 4.0;

const SNAP_MARKER_PIXELS: f64 = 7.0;

/// The glyph of a snap mode as pen strokes, in units of the marker's
/// half-size with +Y up. The centre and tangent snaps also draw a circle
/// (see [`marker_has_circle`]).
///
/// | Mode | Marker |
/// | --- | --- |
/// | Endpoint | square |
/// | Midpoint | triangle |
/// | Center | circle |
/// | Quadrant | diamond |
/// | Intersection | cross |
/// | Nearest | hourglass |
/// | Perpendicular | right-angle sign |
/// | Tangent | circle under a bar |
/// | Insertion | two squares, offset |
/// | Node | a circle with an X through it |
fn marker_strokes(mode: SnapMode) -> &'static [&'static [(f64, f64)]] {
    const SQUARE: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (1., 1.), (-1., 1.), (-1., -1.)];
    const TRIANGLE: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (0., 1.), (-1., -1.)];
    const DIAMOND: &[(f64, f64)] = &[(0., -1.), (1., 0.), (0., 1.), (-1., 0.), (0., -1.)];
    const HOURGLASS: &[(f64, f64)] = &[(-1., -1.), (1., -1.), (-1., 1.), (1., 1.), (-1., -1.)];
    const RISING: &[(f64, f64)] = &[(-1., -1.), (1., 1.)];
    const FALLING: &[(f64, f64)] = &[(-1., 1.), (1., -1.)];
    const CORNER: &[(f64, f64)] = &[(-1., 1.), (-1., -1.), (1., -1.)];
    const SQUARE_MARK: &[(f64, f64)] = &[(-1., 0.), (0., 0.), (0., -1.)];
    const BAR: &[(f64, f64)] = &[(-1., 1.), (1., 1.)];
    const LOWER_LEFT: &[(f64, f64)] = &[(-1., -1.), (0.4, -1.), (0.4, 0.4), (-1., 0.4), (-1., -1.)];
    const UPPER_RIGHT: &[(f64, f64)] =
        &[(-0.4, 1.), (-0.4, -0.4), (1., -0.4), (1., 1.), (-0.4, 1.)];
    match mode {
        SnapMode::Endpoint => &[SQUARE],
        SnapMode::Midpoint => &[TRIANGLE],
        SnapMode::Center => &[],
        SnapMode::Quadrant => &[DIAMOND],
        SnapMode::Intersection => &[RISING, FALLING],
        SnapMode::Nearest => &[HOURGLASS],
        SnapMode::Perpendicular => &[CORNER, SQUARE_MARK],
        SnapMode::Tangent => &[BAR],
        SnapMode::Insertion => &[LOWER_LEFT, UPPER_RIGHT],
        SnapMode::Node => &[RISING, FALLING],
    }
}

/// A constraint's sign inside its glyph's box, as pen strokes and circles
/// in units of the box's half-size with +Y up.
///
/// | Constraint | Sign |
/// | --- | --- |
/// | Horizontal, vertical | a level, a plumb bar |
/// | Parallel | two slanted bars |
/// | Perpendicular | an upturned T |
/// | Collinear | two dashes on one slant |
/// | Concentric | two circles about one centre |
/// | Coincident | a small square |
/// | Tangent | a circle under a bar |
/// | Symmetric | a pair of brackets |
/// | Equal | an equals sign |
/// | Fix | a padlock |
type Strokes = &'static [&'static [(f64, f64)]];
type Circles = &'static [((f64, f64), f64)];

fn glyph_strokes(glyph: Glyph) -> (Strokes, Circles) {
    match glyph {
        Glyph::Horizontal => (&[&[(-0.6, 0.0), (0.6, 0.0)]], &[]),
        Glyph::Vertical => (&[&[(0.0, -0.6), (0.0, 0.6)]], &[]),
        Glyph::Parallel => (
            &[
                &[(-0.55, -0.55), (0.05, 0.55)],
                &[(-0.05, -0.55), (0.55, 0.55)],
            ],
            &[],
        ),
        Glyph::Perpendicular => (
            &[&[(-0.6, -0.5), (0.6, -0.5)], &[(0.0, -0.5), (0.0, 0.6)]],
            &[],
        ),
        Glyph::Collinear => (
            &[&[(-0.65, -0.4), (-0.2, -0.12)], &[(0.2, 0.12), (0.65, 0.4)]],
            &[],
        ),
        Glyph::Concentric => (&[], &[((0.0, 0.0), 0.6), ((0.0, 0.0), 0.25)]),
        Glyph::Coincident => (
            &[&[
                (-0.25, -0.25),
                (0.25, -0.25),
                (0.25, 0.25),
                (-0.25, 0.25),
                (-0.25, -0.25),
            ]],
            &[],
        ),
        Glyph::Tangent => (&[&[(-0.65, 0.35), (0.65, 0.35)]], &[((0.0, -0.15), 0.5)]),
        Glyph::Symmetric => (
            &[
                &[(-0.2, -0.6), (-0.55, -0.6), (-0.55, 0.6), (-0.2, 0.6)],
                &[(0.2, -0.6), (0.55, -0.6), (0.55, 0.6), (0.2, 0.6)],
            ],
            &[],
        ),
        Glyph::Equal => (
            &[
                &[(-0.55, 0.25), (0.55, 0.25)],
                &[(-0.55, -0.25), (0.55, -0.25)],
            ],
            &[],
        ),
        Glyph::Fix => (
            &[
                &[
                    (-0.45, -0.6),
                    (0.45, -0.6),
                    (0.45, 0.05),
                    (-0.45, 0.05),
                    (-0.45, -0.6),
                ],
                &[(-0.25, 0.05), (-0.25, 0.5), (0.25, 0.5), (0.25, 0.05)],
            ],
            &[],
        ),
    }
}

/// Whether a snap mode's glyph includes a circle the size of the marker.
fn marker_has_circle(mode: SnapMode) -> bool {
    matches!(mode, SnapMode::Center | SnapMode::Tangent | SnapMode::Node)
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

/// On-screen radius, in CSS pixels, past which a committed circle or arc
/// is drawn as the visible span of its curve rather than by the analytic
/// shader (#112): `f32` coarsens the stroke from 10⁵ pixels and loses it
/// near 10⁷ (ADR-0009). The adapter collapses such instances at a radius
/// one per cent larger, so the band between is drawn twice, never not at all.
pub const FAR_CURVE_RADIUS_PIXELS: f64 = 50_000.0;
/// How many entities with a curve large enough to be far are read one by
/// one before the view's own query is the cheaper way (#210).
const FAR_CURVE_CANDIDATES: usize = 1024;
/// Chord error of a far curve's segments, in CSS pixels.
const FAR_CURVE_CHORD_PIXELS: f64 = 0.25;
/// How far outside the view a far curve is still drawn, in CSS pixels.
const FAR_CURVE_MARGIN_PIXELS: f64 = 16.0;

/// The pieces of the angular interval `[a, b]` (b − a ≤ 2π) that lie on
/// the arc from `start` over `sweep`, as angle intervals on that arc: two
/// when the interval wraps past the arc's end and back in at its start.
fn arc_pieces(a: f64, b: f64, start: f64, sweep: f64) -> Vec<(f64, f64)> {
    // A full circle has no ends to clip at.
    if sweep >= TAU {
        return vec![(a, b)];
    }
    let from = (a - start).rem_euclid(TAU);
    let to = from + (b - a);
    let mut pieces = Vec::new();
    for offset in [0.0, -TAU] {
        let (lo, hi) = ((from + offset).max(0.0), (to + offset).min(sweep));
        if hi > lo {
            pieces.push((start + lo, start + hi));
        }
    }
    pieces
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
        self.rebuild_in(items, drawing, LayoutId::MODEL, view);
    }

    /// [`rebuild`](Self::rebuild) for the session in `space` (#193): the
    /// far curves it draws are the space's.
    pub fn rebuild_in(
        &mut self,
        items: &[OverlayItem],
        drawing: &Drawing,
        space: LayoutId,
        view: &View,
    ) {
        // The overlay is rebuilt freely, so it can always sit on the view centre.
        self.frame.reset(view.center());
        self.push_far_curves(drawing, space, view);
        self.push_construction_lines(drawing, space, view);
        self.push_construction_items(items, drawing, view);
        for item in items {
            match item {
                OverlayItem::Grid { spacing } => self.push_grid(*spacing, view),
                OverlayItem::Highlight(id) => {
                    if let Some(entity) = drawing.entity(*id) {
                        self.frame
                            .push_entity(StyleRole::Selection, entity, drawing.tables());
                    }
                }
                OverlayItem::Hover(id) => {
                    if let Some(entity) = drawing.entity(*id) {
                        self.frame
                            .push_entity(StyleRole::Hover, entity, drawing.tables());
                    }
                }
                OverlayItem::PreviewRemoval(entity) => {
                    self.frame
                        .push_entity(StyleRole::Removal, entity, drawing.tables());
                }
                OverlayItem::Grips { at } => {
                    for grip in at {
                        self.push_square(StyleRole::Grip, *grip, GRIP_PIXELS, view);
                    }
                }
                OverlayItem::HotGrip { at } => {
                    self.push_square(StyleRole::GripHot, *at, HOT_GRIP_PIXELS, view);
                }
                OverlayItem::PreviewGrip { id, to } => {
                    let preview = drawing
                        .entity(id.entity)
                        .and_then(|entity| entity.grip_preview(id.index, *to));
                    if let Some(preview) = preview {
                        self.frame
                            .push_entity(StyleRole::Preview, &preview, drawing.tables());
                    }
                }
                OverlayItem::PreviewSegment { start, end } => {
                    self.frame.push_segment(StyleRole::Preview, *start, *end);
                }
                OverlayItem::PreviewEntity(entity) => {
                    self.frame
                        .push_entity(StyleRole::Preview, entity, drawing.tables())
                }
                OverlayItem::ProposalAdded(entity) => {
                    self.frame
                        .push_entity(StyleRole::Proposal, entity, drawing.tables())
                }
                OverlayItem::ProposalRemoved(entity) => {
                    self.frame
                        .push_entity(StyleRole::Removal, entity, drawing.tables())
                }
                OverlayItem::PreviewTransformed { ids, transform } => {
                    for entity in ids.iter().filter_map(|id| drawing.entity(*id)) {
                        self.frame.push_entity(
                            StyleRole::Preview,
                            &entity.transformed(transform),
                            drawing.tables(),
                        );
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
                OverlayItem::SelectionPath {
                    points,
                    to,
                    closed,
                    crossing,
                } => {
                    let style = if *crossing {
                        StyleRole::SelectionCrossing
                    } else {
                        StyleRole::SelectionWindow
                    };
                    let mut path: Vec<Point2> = points.clone();
                    path.push(*to);
                    for pair in path.windows(2) {
                        self.frame.push_segment(style, pair[0], pair[1]);
                    }
                    if *closed && path.len() > 2 {
                        self.frame.push_segment(style, *to, path[0]);
                    }
                }
                OverlayItem::TrackingLine { from, through } => {
                    self.push_tracking_line(*from, *through, view)
                }
                OverlayItem::AcquiredPoint { at } => self.push_acquired(*at, view),
                OverlayItem::SnapMarker { mode, at } => self.push_snap_marker(*mode, *at, view),
                OverlayItem::Crosshair { at } => self.push_crosshair(*at, view),
                OverlayItem::ViewportFrame { corners } => {
                    for (index, corner) in corners.iter().enumerate() {
                        self.frame.push_segment(
                            StyleRole::SelectionWindow,
                            *corner,
                            corners[(index + 1) % corners.len()],
                        );
                    }
                }
                OverlayItem::ConstraintBar { at, glyphs, hot } => {
                    for (index, glyph) in glyphs.iter().enumerate() {
                        let style = if *hot == Some(index) {
                            StyleRole::GripHot
                        } else {
                            StyleRole::Constraint
                        };
                        self.push_glyph(*glyph, *at, index, style, view);
                    }
                }
                OverlayItem::ConstraintDimension(entity) => {
                    self.frame
                        .push_entity(StyleRole::Constraint, entity, drawing.tables());
                }
            }
        }
    }

    /// Glyph `index` of a constraint bar anchored at `at` (#269): its box
    /// and its sign, sized in pixels by `glyph_square`.
    fn push_glyph(
        &mut self,
        glyph: Glyph,
        at: Point2,
        index: usize,
        style: StyleRole,
        view: &View,
    ) {
        let (low, high) = glyph_square(view, at, index);
        let a = view.screen_to_world(low);
        let b = view.screen_to_world(high);
        let (left, right) = (a.x.min(b.x), a.x.max(b.x));
        let (bottom, top) = (a.y.min(b.y), a.y.max(b.y));
        let corners = [
            Point2::new(left, bottom),
            Point2::new(right, bottom),
            Point2::new(right, top),
            Point2::new(left, top),
        ];
        for (i, corner) in corners.iter().enumerate() {
            self.frame
                .push_segment(style, *corner, corners[(i + 1) % 4]);
        }
        let center = Point2::new((left + right) / 2.0, (bottom + top) / 2.0);
        let half = (right - left) / 2.0;
        let place = |(x, y): (f64, f64)| center + Vec2::new(x * half, y * half);
        let (strokes, circles) = glyph_strokes(glyph);
        for stroke in strokes {
            for pair in stroke.windows(2) {
                self.frame
                    .push_segment(style, place(pair[0]), place(pair[1]));
            }
        }
        for &((x, y), radius) in circles {
            let circle = Arc::new(place((x, y)), radius * half, 0.0, TAU);
            self.frame.push_arc(style, &circle);
        }
    }

    /// The committed curves too large on screen for the analytic shader
    /// (#112; ADR-0009): every circle and arc in view whose on-screen radius
    /// is past [`FAR_CURVE_RADIUS_PIXELS`] has the part of it that crosses
    /// the view tessellated here, in `f64` from the view centre, as
    /// segments; the shader draws nothing for such an instance. The
    /// committed scene is untouched, so pan and zoom upload nothing.
    fn push_far_curves(&mut self, drawing: &Drawing, space: LayoutId, view: &View) {
        let scale = view.scale();
        // A margin, so a stroke whose curve passes just outside is drawn.
        let bounds = view
            .visible_bounds()
            .expanded(view.pixels_to_world(FAR_CURVE_MARGIN_PIXELS));
        // Only a curve larger than this, in drawing units, can be far
        // (#210). Zoomed out, few are or none, and the drawing's list of
        // curves by size names them; zoomed in, the view holds little, and
        // the index names what is in it.
        let threshold = FAR_CURVE_RADIUS_PIXELS / scale;
        let mut large: Vec<_> = drawing
            .curves_larger_than(threshold)
            .take(FAR_CURVE_CANDIDATES + 1)
            .collect();
        let candidates = if large.len() > FAR_CURVE_CANDIDATES {
            drawing.query_shown_in(space, &bounds)
        } else {
            large.retain(|id| {
                drawing.is_shown_in(*id, space)
                    && drawing
                        .entity(*id)
                        .is_some_and(|entity| drawing.bounds_of(entity).intersects(&bounds))
            });
            large
        };
        for id in candidates {
            let Some(entity) = drawing.entity(id) else {
                continue;
            };
            for primitive in entity.primitives(drawing.tables()) {
                let arc = match primitive {
                    Primitive::Circle(circle) => Arc {
                        center: circle.center,
                        radius: circle.radius,
                        start: 0.0,
                        sweep: TAU,
                    },
                    Primitive::Arc(arc) => arc,
                    Primitive::Segment(_) => continue,
                };
                if arc.radius * scale > FAR_CURVE_RADIUS_PIXELS {
                    self.push_far_arc(&arc, view, &bounds);
                }
            }
        }
    }

    /// The construction lines of the space, clipped to the view (ADR-0031):
    /// drawn in the theme's entity colour, as far curves are, since the
    /// overlay has no style table.
    fn push_construction_lines(&mut self, drawing: &Drawing, space: LayoutId, view: &View) {
        let bounds = view
            .visible_bounds()
            .expanded(view.pixels_to_world(FAR_CURVE_MARGIN_PIXELS));
        for (_, entity) in drawing.unbounded_shown_in(space) {
            self.push_construction(StyleRole::Entity, entity, &bounds);
        }
    }

    /// An xline or a ray, the part of it within `bounds`, in `role`.
    fn push_construction(&mut self, role: StyleRole, entity: &Entity, bounds: &Aabb) {
        let piece = match entity {
            Entity::XLine(line) => line.clipped(bounds),
            Entity::Ray(ray) => ray.clipped(bounds),
            _ => None,
        };
        if let Some(piece) = piece {
            self.frame.push_segment(role, piece.start, piece.end);
        }
    }

    /// The construction lines among the items — highlighted, hovered,
    /// previewed — which the walk leaves out, clipped to the view.
    fn push_construction_items(&mut self, items: &[OverlayItem], drawing: &Drawing, view: &View) {
        let bounds = view
            .visible_bounds()
            .expanded(view.pixels_to_world(FAR_CURVE_MARGIN_PIXELS));
        for item in items {
            match item {
                OverlayItem::Highlight(id) | OverlayItem::Hover(id) => {
                    let role = if matches!(item, OverlayItem::Highlight(_)) {
                        StyleRole::Selection
                    } else {
                        StyleRole::Hover
                    };
                    if let Some(entity) = drawing.entity(*id) {
                        self.push_construction(role, entity, &bounds);
                    }
                }
                OverlayItem::PreviewRemoval(entity) => {
                    self.push_construction(StyleRole::Removal, entity, &bounds);
                }
                OverlayItem::PreviewEntity(entity) => {
                    self.push_construction(StyleRole::Preview, entity, &bounds);
                }
                OverlayItem::ProposalAdded(entity) => {
                    self.push_construction(StyleRole::Proposal, entity, &bounds);
                }
                OverlayItem::ProposalRemoved(entity) => {
                    self.push_construction(StyleRole::Removal, entity, &bounds);
                }
                OverlayItem::PreviewTransformed { ids, transform } => {
                    for entity in ids.iter().filter_map(|id| drawing.entity(*id)) {
                        self.push_construction(
                            StyleRole::Preview,
                            &entity.transformed(transform),
                            &bounds,
                        );
                    }
                }
                OverlayItem::PreviewGrip { id, to } => {
                    if let Some(preview) = drawing
                        .entity(id.entity)
                        .and_then(|entity| entity.grip_preview(id.index, *to))
                    {
                        self.push_construction(StyleRole::Preview, &preview, &bounds);
                    }
                }
                _ => {}
            }
        }
    }

    /// The part of `arc` within reach of the view, as segments with a chord
    /// error under [`FAR_CURVE_CHORD_PIXELS`].
    fn push_far_arc(&mut self, arc: &Arc, view: &View, bounds: &Aabb) {
        let to_view = bounds.center() - arc.center;
        let distance = to_view.length();
        let reach = bounds.min().distance_to(bounds.max()) / 2.0;
        if distance - reach > arc.radius || distance + reach < arc.radius {
            return; // the curve passes nowhere near the view
        }
        // The curve within the view lies within this angle of the direction
        // of the view from the centre: the chord's angle, with room.
        let half = if reach >= arc.radius {
            PI
        } else {
            ((reach / arc.radius).asin() * 1.5).min(PI)
        };
        let mid = to_view.y.atan2(to_view.x);
        let step = (8.0 * FAR_CURVE_CHORD_PIXELS / view.scale() / arc.radius)
            .sqrt()
            .max(1e-9);
        for (from, to) in arc_pieces(mid - half, mid + half, arc.start, arc.sweep) {
            let count = ((to - from) / step).ceil().clamp(1.0, 100_000.0) as usize;
            let at = |i: usize| {
                let angle = from + (to - from) * (i as f64 / count as f64);
                arc.center + Vec2::from_polar(arc.radius, angle)
            };
            for i in 0..count {
                self.frame.push_segment(StyleRole::Entity, at(i), at(i + 1));
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

    /// An axis-aligned square outline centred on `at`, sized in pixels.
    fn push_square(&mut self, style: StyleRole, at: Point2, half_pixels: f64, view: &View) {
        let half = view.pixels_to_world(half_pixels);
        let corners = [
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ];
        for (index, corner) in corners.iter().enumerate() {
            let next = corners[(index + 1) % corners.len()];
            self.frame.push_segment(style, at + *corner, at + next);
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

    /// A point object snap tracking runs from (#220): a small cross, in
    /// the tracking line's role, the same size at any zoom.
    fn push_acquired(&mut self, at: Point2, view: &View) {
        let half = view.pixels_to_world(ACQUIRED_MARK_PIXELS);
        self.frame.push_segment(
            StyleRole::Tracking,
            at + Vec2::new(-half, 0.0),
            at + Vec2::new(half, 0.0),
        );
        self.frame.push_segment(
            StyleRole::Tracking,
            at + Vec2::new(0.0, -half),
            at + Vec2::new(0.0, half),
        );
    }

    /// The marker is sized in pixels here, so it looks the same at any zoom.
    fn push_snap_marker(&mut self, mode: SnapMode, at: Point2, view: &View) {
        let half = view.pixels_to_world(SNAP_MARKER_PIXELS);
        if marker_has_circle(mode) {
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
        h.click(430.0, 300.0);

        let overlay = overlay_of(&h);
        assert_eq!(
            styles(&overlay),
            vec![StyleRole::Selection, StyleRole::Grip, StyleRole::Crosshair]
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

    // ── far curves (#112) ──

    /// A view of `scale` pixels per unit centred on `at`.
    fn view_at(at: Point2, scale: f64) -> View {
        let mut view = View::new(800.0, 600.0);
        view.zoom_about(ScreenPoint::new(400.0, 300.0), scale);
        let delta = at - view.center();
        view.pan_by_pixels(-delta.x * scale, delta.y * scale);
        view
    }

    fn far_segments(drawing: &Drawing, view: &View) -> Vec<(Point2, Point2)> {
        let mut overlay = OverlayFrame::new();
        overlay.rebuild(&[], drawing, view);
        let origin = overlay.frame().origin();
        overlay
            .frame()
            .segments()
            .chunks(PackedFrame::FLOATS_PER_SEGMENT)
            .map(|s| {
                (
                    origin + Vec2::new(f64::from(s[0]), f64::from(s[1])),
                    origin + Vec2::new(f64::from(s[2]), f64::from(s[3])),
                )
            })
            .collect()
    }

    #[test]
    fn a_circle_past_the_shader_radius_is_drawn_as_its_visible_span() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", "100"]);
        let drawing = h.session.drawing();
        // Radius 100 at 10,000 px per unit: a million pixels, looking at the
        // rightmost point.
        let view = view_at(Point2::new(100.0, 0.0), 10_000.0);
        let segments = far_segments(drawing, &view);
        assert!(!segments.is_empty());
        let visible = view.visible_bounds();
        let (mut lowest, mut highest) = (f64::INFINITY, f64::NEG_INFINITY);
        for (a, b) in &segments {
            for point in [a, b] {
                let off = (point.distance_to(Point2::ORIGIN) - 100.0).abs();
                assert!(off < 1e-9, "{point:?} is {off} off the circle");
                lowest = lowest.min(point.y);
                highest = highest.max(point.y);
            }
        }
        assert!(
            lowest <= visible.min().y && highest >= visible.max().y,
            "the span crosses the view"
        );
        // The chords are too short to see: under a quarter pixel from the curve.
        for (a, b) in &segments {
            let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let sagitta = 100.0 - mid.distance_to(Point2::ORIGIN);
            assert!(sagitta * 10_000.0 < 0.3, "sagitta {sagitta}");
        }

        // Below the threshold, the shader draws it: nothing here.
        let near = view_at(Point2::new(100.0, 0.0), 100.0);
        assert!(far_segments(drawing, &near).is_empty());
        // Looking elsewhere, at the same zoom: the curve passes nowhere near.
        let away = view_at(Point2::new(0.0, 0.0), 10_000.0);
        assert!(far_segments(drawing, &away).is_empty());
    }

    #[test]
    fn an_arc_and_a_polyline_bulge_are_drawn_only_where_they_run() {
        let mut h = Harness::new();
        // A quarter arc on the right, from 0° to 90°.
        h.enter_all(&["A", "100,0", "70.71,70.71", "0,100"]);
        // A polyline with a semicircular bulge about (300, 0), to one side.
        h.enter_all(&["PL", "300,-100", "A", "300,100", "L", "", ""]);
        let drawing = h.session.drawing();
        let scale = 10_000.0;
        // At the arc's start: drawn.
        assert!(!far_segments(drawing, &view_at(Point2::new(100.0, 0.0), scale)).is_empty());
        // Where the circle would continue past the arc's end: not drawn.
        assert!(far_segments(drawing, &view_at(Point2::new(70.71, -70.71), scale)).is_empty());
        // The bulge: drawn on whichever side it bulges, on the circle of radius 100.
        let sides = [Point2::new(200.0, 0.0), Point2::new(400.0, 0.0)];
        let drawn: Vec<Vec<(Point2, Point2)>> = sides
            .iter()
            .map(|at| far_segments(drawing, &view_at(*at, scale)))
            .collect();
        assert_eq!(
            drawn.iter().filter(|segments| !segments.is_empty()).count(),
            1
        );
        for (a, _) in drawn.iter().flatten() {
            assert!((a.distance_to(Point2::new(300.0, 0.0)) - 100.0).abs() < 1e-9);
        }
    }

    #[test]
    fn arc_pieces_follow_the_arc_and_wrap() {
        // A full circle takes the interval as it is.
        assert_eq!(arc_pieces(-0.5, 0.5, 0.0, TAU), vec![(-0.5, 0.5)]);
        // An interval inside the arc.
        assert_eq!(arc_pieces(1.0, 2.0, 0.5, 3.0), vec![(1.0, 2.0)]);
        // An interval crossing the arc's start is clipped to it.
        assert_eq!(arc_pieces(0.0, 1.0, 0.5, 3.0), vec![(0.5, 1.0)]);
        // An interval wholly off the arc.
        assert_eq!(arc_pieces(4.0, 5.0, 0.5, 3.0), vec![]);
        // An interval wrapping round: past the arc's end and back in at its start.
        let pieces = arc_pieces(3.0, 7.0, 0.5, 3.0);
        assert_eq!(pieces.len(), 2);
        assert!((pieces[0].0 - 3.0).abs() < 1e-12 && (pieces[0].1 - 3.5).abs() < 1e-12);
        assert!((pieces[1].0 - 0.5).abs() < 1e-12 && (pieces[1].1 - (7.0 - TAU)).abs() < 1e-12);
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

        // The two constructions: a right-angle sign, and a circle under a bar.
        let perpendicular = marker_frame(SnapMode::Perpendicular, &view);
        assert_eq!(
            (
                perpendicular.frame().arc_count(),
                perpendicular.frame().segment_count()
            ),
            (0, 4)
        );
        let tangent = marker_frame(SnapMode::Tangent, &view);
        assert_eq!(
            (tangent.frame().arc_count(), tangent.frame().segment_count()),
            (1, 1)
        );
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
                StyleRole::Grip,
                StyleRole::Crosshair
            ]
        );
    }

    // ── grips ──

    /// The segments of one style, as `[x0, y0, x1, y1]` in world coordinates.
    fn segments_of(overlay: &OverlayFrame, style: StyleRole) -> Vec<[f64; 4]> {
        let frame = overlay.frame();
        let origin = frame.origin();
        let all = frame.segments().as_chunks::<4>().0;
        frame
            .batches()
            .iter()
            .filter(|batch| batch.style == style)
            .flat_map(|batch| &all[batch.first as usize..(batch.first + batch.count) as usize])
            .map(|segment| {
                let [x0, y0, x1, y1] = segment.map(f64::from);
                [origin.x + x0, origin.y + y0, origin.x + x1, origin.y + y1]
            })
            .collect()
    }

    fn width_of(segments: &[[f64; 4]]) -> f64 {
        let xs = segments.iter().flat_map(|segment| [segment[0], segment[2]]);
        let (low, high) = xs.fold((f64::MAX, f64::MIN), |(low, high), x| {
            (low.min(x), high.max(x))
        });
        high - low
    }

    #[test]
    fn grips_are_squares_of_a_fixed_pixel_size_at_any_zoom() {
        let item = OverlayItem::Grips {
            at: vec![Point2::new(10.0, 10.0), Point2::new(50.0, 10.0)],
        };
        let mut zoomed = view();
        zoomed.zoom_about(ScreenPoint::new(400.0, 300.0), 12.5);
        for view in [view(), zoomed] {
            let mut overlay = OverlayFrame::new();
            overlay.rebuild(std::slice::from_ref(&item), &Drawing::new(), &view);
            let squares = segments_of(&overlay, StyleRole::Grip);
            assert_eq!(squares.len(), 8, "four sides per grip");
            let pixels = width_of(&squares[..4]) * view.scale();
            assert!((pixels - 2.0 * GRIP_PIXELS).abs() < 1e-3, "{pixels}px wide");
        }
    }

    #[test]
    fn the_hot_grip_is_drawn_over_the_others_in_its_own_style_and_a_little_larger() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "100,0", ""]);
        h.click_world(Point2::new(30.0, 0.0));
        h.move_to_world(Point2::new(99.0, 1.0));

        let overlay = overlay_of(&h);
        assert_eq!(
            styles(&overlay),
            vec![
                StyleRole::Selection,
                StyleRole::Grip,
                StyleRole::GripHot,
                StyleRole::Crosshair
            ]
        );
        let hot = segments_of(&overlay, StyleRole::GripHot);
        assert_eq!(hot.len(), 4);
        assert!((width_of(&hot) - 2.0 * HOT_GRIP_PIXELS).abs() < 1e-3);
    }

    /// #90: previewing a vertex drag draws the two segments that change,
    /// not the polyline again.
    #[test]
    fn dragging_a_vertex_of_a_huge_polyline_previews_only_what_changes() {
        let vertices = (0..10_000).map(|index| {
            let x = f64::from(index) * 0.05 - 250.0;
            Point2::new(x, if index % 2 == 0 { 0.0 } else { 5.0 })
        });
        let polyline = cad_document::PolylineEntity::new(vertices.collect(), false);
        let mut h = Harness::with_entities([cad_document::Entity::from(polyline)]);
        h.click_world(Point2::new(-250.0, 0.0));
        assert_eq!(h.session.grips().len(), 10_000);

        // Take the first vertex's grip and drag it.
        h.click_world(Point2::new(-250.0, 0.0));
        h.move_to_world(Point2::new(-260.0, 40.0));
        let overlay = overlay_of(&h);
        // The rubber band from the grip's old position, plus one segment to
        // the only neighbour.
        assert_eq!(segments_of(&overlay, StyleRole::Preview).len(), 2);
        assert_eq!(segments_of(&overlay, StyleRole::Grip).len(), 40_000);
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
    fn object_tracking_marks_its_points_and_draws_its_paths_without_touching_the_scene() {
        let mut h = Harness::with_entities([cad_document::LineEntity::new(
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
        )
        .into()]);
        h.toggle(cad_commands::Toggle::Otrack);
        h.enter("L");
        let revision = h.session.drawing().revision();
        h.move_to_world(Point2::new(100.0, 0.0));
        h.session.pointer_rest();
        h.move_to_world(Point2::new(200.0, 3.0));
        let overlay = overlay_of(&h);
        // The acquired end's cross, and the path from it.
        assert_eq!(segments_of(&overlay, StyleRole::Tracking).len(), 3);
        assert_eq!(
            h.session.drawing().revision(),
            revision,
            "nothing committed"
        );
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
