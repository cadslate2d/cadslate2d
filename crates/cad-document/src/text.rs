//! The single-line text entity (DESIGN §11, §32; ADR-0016).
//!
//! A text is a string at an insertion point, with a height, a rotation,
//! a style, and a justification. Its geometry is the strokes of the stroke
//! font laid along its baseline — what the renderer draws — but it is
//! picked and selected by its *box*: the rectangle its glyphs sit in, so
//! that a click inside a letter's counter picks the text. It offers no
//! primitives: nothing intersects, trims against, or snaps to the strokes
//! of a text, only to its insertion point.

use cad_core::tolerance::AngularTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Segment, Transform};
use cad_text::Font;

use crate::text_style::TextStyleId;

/// Where a text's insertion point is relative to its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Justification {
    /// The insertion point is the start of the baseline.
    #[default]
    Left,
    /// The insertion point is the middle of the baseline.
    Center,
    /// The insertion point is the end of the baseline.
    Right,
}

impl Justification {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    /// How far along the baseline the start is from the insertion point,
    /// as a fraction of the width: 0, −½, −1.
    fn start_fraction(self) -> f64 {
        match self {
            Self::Left => 0.0,
            Self::Center => -0.5,
            Self::Right => -1.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
}

/// A single line of text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEntity {
    pub insertion: Point2,
    /// The cap height, in drawing units. Positive.
    pub height: f64,
    /// The baseline's angle from +X, in radians.
    pub rotation: f64,
    pub content: String,
    pub style: TextStyleId,
    pub justification: Justification,
}

/// The box a text's glyphs sit in: its baseline start, and the unit
/// vectors along and up from it, with its width and how far the font
/// reaches above and below the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFrame {
    pub start: Point2,
    pub along: Vec2,
    pub up: Vec2,
    pub width: f64,
    /// How far above the baseline the box reaches: the cap height, and a
    /// little more for brackets.
    pub ascent: f64,
    /// How far below the baseline the box reaches.
    pub descent: f64,
}

impl TextFrame {
    /// The four corners: bottom start and end, then the top end and the
    /// top start — counter-clockwise.
    pub fn corners(&self) -> [Point2; 4] {
        let bottom = self.start - self.up * self.descent;
        let right = self.along * self.width;
        let up = self.up * (self.ascent + self.descent);
        [bottom, bottom + right, bottom + right + up, bottom + up]
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

    /// Whether `point` is in the box.
    pub fn contains(&self, point: Point2) -> bool {
        let local = point - self.start;
        let x = local.dot(self.along);
        let y = local.dot(self.up);
        (0.0..=self.width).contains(&x) && (-self.descent..=self.ascent).contains(&y)
    }
}

/// `angle` brought into (−π, π].
pub(crate) fn signed_angle(angle: f64) -> f64 {
    let turned = angle.rem_euclid(std::f64::consts::TAU);
    if turned > std::f64::consts::PI {
        turned - std::f64::consts::TAU
    } else {
        turned
    }
}

impl TextEntity {
    pub fn new(insertion: Point2, height: f64, rotation: f64, content: impl Into<String>) -> Self {
        Self {
            insertion,
            height,
            rotation,
            content: content.into(),
            style: TextStyleId::FIRST,
            justification: Justification::Left,
        }
    }

    #[must_use]
    pub fn justified(self, justification: Justification) -> Self {
        Self {
            justification,
            ..self
        }
    }

    /// The same text saying something else.
    #[must_use]
    pub fn saying(&self, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..self.clone()
        }
    }

    /// The width of the text, in drawing units, by the font's metrics.
    pub fn width(&self) -> f64 {
        Font::simplex().advance(&self.content) * self.height
    }

    /// Where the baseline starts: the insertion point, less the
    /// justification's share of the width.
    pub fn start(&self) -> Point2 {
        self.insertion + self.along() * (self.justification.start_fraction() * self.width())
    }

    fn along(&self) -> Vec2 {
        Vec2::new(self.rotation.cos(), self.rotation.sin())
    }

    /// The box the glyphs sit in.
    pub fn frame(&self) -> TextFrame {
        let along = self.along();
        let font = Font::simplex();
        TextFrame {
            start: self.start(),
            along,
            up: Vec2::new(-along.y, along.x),
            width: self.width(),
            ascent: font.ascent() * self.height,
            descent: font.descent() * self.height,
        }
    }

    /// The strokes that draw the text: polylines in drawing units.
    pub fn strokes(&self) -> Vec<Vec<Point2>> {
        Font::simplex().strokes(&self.content, self.start(), self.height, self.rotation)
    }

    pub fn bounds(&self) -> Aabb {
        let [a, b, c, d] = self.frame().corners();
        [b, c, d]
            .iter()
            .fold(Aabb::from_points(a, a), |bounds, corner| {
                bounds.union(&Aabb::from_points(*corner, *corner))
            })
    }

    /// Zero inside the box; outside, the distance to its nearest edge.
    pub fn distance_to(&self, point: Point2) -> f64 {
        let frame = self.frame();
        if frame.contains(point) {
            return 0.0;
        }
        frame
            .edges()
            .iter()
            .map(|edge| edge.distance_to_point(point))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether any edge of the box lies within `bounds`, or the box holds
    /// the whole of `bounds`: a crossing selection takes the text either way.
    pub fn crosses(&self, bounds: &Aabb) -> bool {
        let frame = self.frame();
        frame
            .edges()
            .iter()
            .any(|edge| edge.intersects_aabb(bounds))
            || frame.contains(bounds.min())
    }

    /// The text moved, turned, scaled, or mirrored. A mirrored text keeps
    /// reading the right way: it takes the mirrored box and lays itself in
    /// it from whichever end leaves the baseline the right way up, as a
    /// draughtsman would redraw it (DESIGN §22).
    pub fn transformed(&self, transform: &Transform) -> Self {
        let height = self.height * transform.scale_factor();
        if !transform.reverses_orientation() {
            return Self {
                insertion: transform.apply(self.insertion),
                height,
                rotation: transform.apply_angle(self.rotation),
                ..self.clone()
            };
        }
        let frame = self.frame();
        let start = transform.apply(frame.start);
        let end = transform.apply(frame.start + frame.along * frame.width);
        let top = transform.apply(frame.start + frame.up * self.height);
        let mut along = transform.apply_angle(self.rotation);
        // Upside down — reading right to left — is turned round: the text
        // runs from the other end of the mirrored baseline. Vertical text
        // is left as it is.
        let cosine = along.cos();
        let upside_down = cosine < 0.0 && !AngularTolerance::DEFAULT.is_zero(cosine);
        let baseline_start = if upside_down {
            along += std::f64::consts::PI;
            end
        } else {
            start
        };
        let along = signed_angle(along);
        let direction = Vec2::new(along.cos(), along.sin());
        let up = Vec2::new(-direction.y, direction.x);
        // The mirrored box's top may now be on the down side of the
        // baseline: then the baseline moves to the top edge, so the glyphs
        // fill the same box.
        let rise = (top - start).dot(up);
        let new_start = if rise < 0.0 {
            baseline_start + up * rise
        } else {
            baseline_start
        };
        let width = frame.width * transform.scale_factor();
        Self {
            insertion: new_start - direction * (self.justification.start_fraction() * width),
            height,
            rotation: along,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    #[test]
    fn the_box_follows_the_justification_height_and_rotation() {
        let text = TextEntity::new(p(10.0, 20.0), 5.0, 0.0, "HI");
        let width = text.width();
        assert!(width > 5.0 && width < 10.0, "two capitals: {width}");
        assert!(close(text.start(), p(10.0, 20.0)));
        let frame = text.frame();
        assert!(
            frame.ascent >= 5.0 && frame.ascent < 6.5,
            "{}",
            frame.ascent
        );
        assert!(frame.contains(p(12.0, 22.0)));
        assert!(!frame.contains(p(12.0, 27.0)));
        assert!(frame.contains(p(12.0, 19.5)), "the descent is in the box");
        let bounds = text.bounds();
        assert!((bounds.min().x - 10.0).abs() < 1e-9);
        assert!((bounds.max().y - (20.0 + frame.ascent)).abs() < 1e-9);

        let centred = text.clone().justified(Justification::Center);
        assert!(close(centred.start(), p(10.0 - width / 2.0, 20.0)));
        let right = text.clone().justified(Justification::Right);
        assert!(close(right.start(), p(10.0 - width, 20.0)));

        let up = TextEntity::new(p(0.0, 0.0), 5.0, FRAC_PI_2, "HI");
        let corners = up.frame().corners();
        assert!(corners.iter().all(|c| c.x <= 1e-9 + up.frame().descent));
        assert!((up.bounds().max().y - width).abs() < 1e-9);
    }

    #[test]
    fn picking_is_by_the_box_and_there_are_no_primitives_to_snap_to() {
        let text = TextEntity::new(p(0.0, 0.0), 10.0, 0.0, "O");
        // Inside the letter's counter, where no stroke is.
        assert_eq!(text.distance_to(p(text.width() / 2.0, 5.0)), 0.0);
        let above = text.frame().ascent + 3.0;
        assert!((text.distance_to(p(text.width() / 2.0, above)) - 3.0).abs() < 1e-9);
        assert!(!text.strokes().is_empty());
        let as_entity = crate::Entity::Text(text.clone());
        assert!(as_entity.primitives(crate::block::no_blocks()).is_empty());
        assert_eq!(
            as_entity.distance_to(p(-4.0, 5.0), crate::block::no_blocks()),
            4.0
        );
    }

    #[test]
    fn selection_boxes_see_the_text_box() {
        let text = TextEntity::new(p(0.0, 0.0), 10.0, 0.0, "AB");
        let width = text.width();
        let around = Aabb::from_points(p(-1.0, -5.0), p(width + 1.0, 13.0));
        let entity = crate::Entity::Text(text);
        assert!(entity.is_inside(&around, crate::block::no_blocks()));
        assert!(entity.crosses(&around, crate::block::no_blocks()));
        let clipping = Aabb::from_points(p(-1.0, 5.0), p(1.0, 6.0));
        assert!(!entity.is_inside(&clipping, crate::block::no_blocks()));
        assert!(
            entity.crosses(&clipping, crate::block::no_blocks()),
            "an edge crosses it"
        );
        let within = Aabb::from_points(p(2.0, 2.0), p(3.0, 3.0));
        assert!(
            entity.crosses(&within, crate::block::no_blocks()),
            "a box inside the text takes it"
        );
        let away = Aabb::from_points(p(-10.0, -10.0), p(-5.0, -5.0));
        assert!(!entity.crosses(&away, crate::block::no_blocks()));
    }

    #[test]
    fn a_text_is_moved_turned_and_scaled_as_a_whole() {
        let text = TextEntity::new(p(10.0, 0.0), 2.0, 0.0, "ab");
        let moved = text.transformed(&Transform::translation(Vec2::new(1.0, 2.0)));
        assert!(close(moved.insertion, p(11.0, 2.0)));
        assert_eq!((moved.height, moved.rotation), (2.0, 0.0));
        let turned = text.transformed(&Transform::rotation(Point2::ORIGIN, 90.0));
        assert!(close(turned.insertion, p(0.0, 10.0)));
        assert!((turned.rotation - FRAC_PI_2).abs() < 1e-9);
        let scaled = text.transformed(&Transform::scaling(Point2::ORIGIN, 3.0).unwrap());
        assert!(close(scaled.insertion, p(30.0, 0.0)));
        assert_eq!(scaled.height, 6.0);
        assert_eq!(scaled.content, "ab");
    }

    #[test]
    fn a_mirrored_text_fills_the_mirrored_box_and_still_reads() {
        let text = TextEntity::new(p(10.0, 0.0), 4.0, 0.0, "AB");
        let width = text.width();
        // Across a vertical axis at x = 0: the box now runs from -10 - w to -10.
        let across = text.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        assert!(
            (across.rotation.rem_euclid(2.0 * PI)).abs() < 1e-9,
            "still reads left to right"
        );
        assert!(close(across.start(), p(-10.0 - width, 0.0)));
        assert_eq!(across.height, 4.0);
        // Across a horizontal axis at y = 0: the box is below, the baseline
        // moves to its top so the glyphs stay the right way up.
        let below = text.transformed(&Transform::mirror(p(0.0, 0.0), p(1.0, 0.0)).unwrap());
        assert!(below.rotation.abs() < 1e-9);
        assert!(close(below.start(), p(10.0, -4.0)));
        // A right-justified text keeps its justification and its box.
        let right = text.clone().justified(Justification::Right);
        let right_across = right.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        assert_eq!(right_across.justification, Justification::Right);
        assert!(close(right_across.start(), p(-10.0, 0.0)));
        // Vertical text is left reading upward.
        let vertical = TextEntity::new(p(0.0, 0.0), 4.0, FRAC_PI_2, "AB");
        let mirrored = vertical.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        assert!((mirrored.rotation - FRAC_PI_2).abs() < 1e-9);
    }

    proptest! {
        /// Bounds hold every stroke and every corner; the box's distance is
        /// zero exactly where a point is inside it; a transform keeps the
        /// content and the height's ratio.
        #[test]
        fn bounds_hold_the_strokes_and_the_box_is_what_is_picked(
            x in -500.0..500.0f64, y in -500.0..500.0f64,
            height in 0.5..50.0f64, rotation in -7.0..7.0f64,
            content in "[ -~]{1,10}",
            justification in prop::sample::select(Justification::ALL.to_vec()),
            px in -600.0..600.0f64, py in -600.0..600.0f64,
        ) {
            let text = TextEntity::new(p(x, y), height, rotation, content.clone()).justified(justification);
            let bounds = text.bounds();
            let loose = Aabb::from_points(
                bounds.min() - Vec2::new(1e-6, 1e-6), bounds.max() + Vec2::new(1e-6, 1e-6));
            for point in text.strokes().iter().flatten() {
                prop_assert!(loose.contains_point(*point), "{point:?} outside {bounds:?}");
            }
            let probe = p(px, py);
            let inside = text.frame().contains(probe);
            prop_assert_eq!(text.distance_to(probe) == 0.0, inside);
            let scaled = text.transformed(&Transform::scaling(p(1.0, 1.0), 2.0).unwrap());
            prop_assert!((scaled.height - 2.0 * height).abs() < 1e-9);
            prop_assert_eq!(&scaled.content, &content);
            let mirrored = text.transformed(&Transform::mirror(p(0.0, 0.0), p(1.0, 1.0)).unwrap());
            prop_assert!(mirrored.rotation.cos() >= -1e-6, "never upside down");
            prop_assert!((mirrored.width() - text.width()).abs() < 1e-9);
        }
    }
}
