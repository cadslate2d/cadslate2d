//! The multiline text entity (DESIGN §11, §32; ADR-0016).
//!
//! A paragraph: a string with explicit line breaks, wrapped at spaces to a
//! width, set in lines from an insertion point at its top left, at a
//! height, a rotation, and a style. Like a single-line text it is drawn
//! as the stroke font's strokes, has no primitives, is picked by its box,
//! and offers only its insertion point to snap to.

use cad_core::tolerance::AngularTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::Transform;
use cad_text::Font;

use crate::text::TextFrame;
use crate::text_style::TextStyleId;

/// Baseline to baseline, in text heights: the spacing drafting programs
/// use by default.
pub const LINE_SPACING: f64 = 5.0 / 3.0;

/// A paragraph of text.
#[derive(Clone, Debug, PartialEq)]
pub struct MTextEntity {
    /// The top left of the first line's cap height.
    pub insertion: Point2,
    /// The width lines are wrapped to, in drawing units; zero wraps
    /// nothing, and only explicit breaks make lines.
    pub width: f64,
    /// The cap height, in drawing units. Positive.
    pub height: f64,
    /// The baseline's angle from +X, in radians.
    pub rotation: f64,
    /// The text, with `\n` where a line must break.
    pub content: String,
    pub style: TextStyleId,
}

impl MTextEntity {
    pub fn new(
        insertion: Point2,
        width: f64,
        height: f64,
        rotation: f64,
        content: impl Into<String>,
    ) -> Self {
        Self {
            insertion,
            width,
            height,
            rotation,
            content: content.into(),
            style: TextStyleId::FIRST,
        }
    }

    /// The same paragraph saying something else.
    #[must_use]
    pub fn saying(&self, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..self.clone()
        }
    }

    /// The lines as they are set: the content wrapped to the width.
    pub fn lines(&self) -> Vec<String> {
        Font::simplex().wrap(&self.content, self.width / self.height)
    }

    fn along(&self) -> Vec2 {
        Vec2::new(self.rotation.cos(), self.rotation.sin())
    }

    fn up(&self) -> Vec2 {
        let along = self.along();
        Vec2::new(-along.y, along.x)
    }

    /// Where line `index` starts: one height below the insertion, then
    /// the line spacing for every line before it.
    fn baseline(&self, index: usize) -> Point2 {
        let down = self.height * (1.0 + LINE_SPACING * index as f64);
        self.insertion - self.up() * down
    }

    /// The box the lines fill: as wide as the wrap width or the widest
    /// line, from the insertion down to the last line's descent.
    pub fn frame(&self) -> TextFrame {
        let font = Font::simplex();
        let lines = self.lines();
        let widest = lines
            .iter()
            .map(|line| font.advance(line) * self.height)
            .fold(0.0, f64::max);
        let last = lines.len().saturating_sub(1);
        let start = self.baseline(last);
        TextFrame {
            start,
            along: self.along(),
            up: self.up(),
            width: self.width.max(widest),
            // Up to the insertion, and a little more for brackets.
            ascent: (self.insertion - start).dot(self.up()) + (font.ascent() - 1.0) * self.height,
            descent: font.descent() * self.height,
        }
    }

    /// The strokes that draw every line: polylines in drawing units.
    pub fn strokes(&self) -> Vec<Vec<Point2>> {
        let font = Font::simplex();
        self.lines()
            .iter()
            .enumerate()
            .flat_map(|(index, line)| {
                font.strokes(line, self.baseline(index), self.height, self.rotation)
            })
            .collect()
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

    pub fn crosses(&self, bounds: &Aabb) -> bool {
        let frame = self.frame();
        frame
            .edges()
            .iter()
            .any(|edge| edge.intersects_aabb(bounds))
            || frame.contains(bounds.min())
    }

    /// The paragraph moved, turned, scaled, or mirrored. A mirrored
    /// paragraph takes the mirrored box and reads the right way up in it,
    /// from its new top left (see `TextEntity::transformed`).
    pub fn transformed(&self, transform: &Transform) -> Self {
        let scale = transform.scale_factor();
        let scaled = Self {
            width: self.width * scale,
            height: self.height * scale,
            ..self.clone()
        };
        if !transform.reverses_orientation() {
            return Self {
                insertion: transform.apply(self.insertion),
                rotation: transform.apply_angle(self.rotation),
                ..scaled
            };
        }
        let mut along = transform.apply_angle(self.rotation);
        let cosine = along.cos();
        if cosine < 0.0 && !AngularTolerance::DEFAULT.is_zero(cosine) {
            along += std::f64::consts::PI;
        }
        let along = crate::text::signed_angle(along);
        let direction = Vec2::new(along.cos(), along.sin());
        let up = Vec2::new(-direction.y, direction.x);
        // The mirrored cap box: the corner furthest up and furthest back
        // along the new direction is the new top left.
        let frame = self.frame();
        let top_left = self.insertion;
        let top_right = top_left + frame.along * frame.width;
        let cap_depth = (self.insertion - frame.start).dot(frame.up);
        let bottom_left = top_left - frame.up * cap_depth;
        let bottom_right = bottom_left + frame.along * frame.width;
        let corners = [top_left, top_right, bottom_left, bottom_right].map(|c| transform.apply(c));
        let insertion = corners
            .into_iter()
            .min_by(|a, b| {
                let key = |p: &Point2| (p.dot_along(direction), -p.dot_along(up));
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("four corners");
        Self {
            insertion,
            rotation: along,
            ..scaled
        }
    }
}

/// A point's coordinate along a unit direction.
trait DotAlong {
    fn dot_along(&self, direction: Vec2) -> f64;
}

impl DotAlong for Point2 {
    fn dot_along(&self, direction: Vec2) -> f64 {
        (*self - Point2::ORIGIN).dot(direction)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    fn note() -> MTextEntity {
        // "one two" fits the width; the rest wraps.
        let width = Font::simplex().advance("one two") * 4.0 + 0.1;
        MTextEntity::new(p(10.0, 50.0), width, 4.0, 0.0, "one two three\nfour")
    }

    #[test]
    fn lines_are_wrapped_to_the_width_and_set_from_the_top_left() {
        let note = note();
        assert_eq!(note.lines(), vec!["one two", "three", "four"]);
        let frame = note.frame();
        // Three lines: the box runs from the insertion down to the last
        // baseline, one height plus two spacings below it.
        let depth = 4.0 * (1.0 + 2.0 * LINE_SPACING);
        let brackets = (Font::simplex().ascent() - 1.0) * 4.0;
        assert!((frame.ascent - depth - brackets).abs() < 1e-9);
        assert!(close(frame.start, p(10.0, 50.0 - depth)));
        assert!(
            (frame.width - note.width).abs() < 1e-9,
            "as wide as the wrap width"
        );
        assert!(frame.contains(p(12.0, 48.0)) && frame.contains(p(12.0, 50.0 - depth + 1.0)));
        assert!(!frame.contains(p(12.0, 52.0)));
        let bounds = note.bounds();
        assert!((bounds.max().y - 50.0 - brackets).abs() < 1e-9);
        assert_eq!(note.distance_to(p(12.0, 48.0)), 0.0);
        assert!((note.distance_to(p(12.0, 53.0 + brackets)) - 3.0).abs() < 1e-9);
        // A line wider than the width widens the box.
        let wide = note.saying("supercalifragilistic");
        assert!(wide.frame().width > note.width);
    }

    #[test]
    fn the_strokes_are_every_lines_glyphs_and_turn_with_the_paragraph() {
        let note = note();
        let flat = note.strokes();
        let one_line = MTextEntity::new(p(10.0, 50.0), 0.0, 4.0, 0.0, "one two three four");
        assert_eq!(
            flat.len(),
            one_line.strokes().len(),
            "the same glyphs, set in lines"
        );
        let turned = MTextEntity {
            rotation: FRAC_PI_2,
            ..note.clone()
        };
        // Turned a quarter: the lines run upward, and successive lines
        // lie further to the right.
        let frame = turned.frame();
        assert!((frame.up - Vec2::new(-1.0, 0.0)).length() < 1e-9);
        assert!(
            turned
                .strokes()
                .iter()
                .flatten()
                .all(|point| point.x >= 10.0 - 1e-9)
        );
        let as_entity = crate::Entity::MText(note.clone());
        assert!(as_entity.primitives().is_empty());
        assert_eq!(as_entity.kind_name(), "multiline text");
    }

    #[test]
    fn a_paragraph_is_transformed_whole_and_mirrored_readable() {
        let note = note();
        let moved = note.transformed(&Transform::translation(Vec2::new(1.0, -1.0)));
        assert!(close(moved.insertion, p(11.0, 49.0)));
        assert_eq!(moved.lines(), note.lines());
        let scaled = note.transformed(&Transform::scaling(p(10.0, 50.0), 2.0).unwrap());
        assert_eq!((scaled.height, scaled.width), (8.0, note.width * 2.0));
        assert_eq!(
            scaled.lines(),
            note.lines(),
            "wrapping scales with the text"
        );
        let turned = note.transformed(&Transform::rotation(p(10.0, 50.0), 90.0));
        assert!((turned.rotation - FRAC_PI_2).abs() < 1e-9);

        // Across a vertical axis at x = 0: the box lands at x in [-10 - w, -10],
        // still reading left to right from its top left.
        let across = note.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        assert!(across.rotation.abs() < 1e-9);
        assert!(close(across.insertion, p(-10.0 - note.frame().width, 50.0)));
        assert_eq!(across.lines(), note.lines());
        // Across a horizontal axis at y = 0: the box is below, top left at
        // the top of the mirrored box.
        let below = note.transformed(&Transform::mirror(p(0.0, 0.0), p(1.0, 0.0)).unwrap());
        assert!(below.rotation.abs() < 1e-9);
        let cap_depth = 4.0 * (1.0 + 2.0 * LINE_SPACING);
        assert!(close(below.insertion, p(10.0, -50.0 + cap_depth)));
    }

    proptest! {
        #[test]
        fn bounds_hold_every_stroke_and_the_box_is_what_is_picked(
            x in -500.0..500.0f64, y in -500.0..500.0f64,
            width in 0.0..200.0f64, height in 0.5..30.0f64, rotation in -7.0..7.0f64,
            content in "[ -~\\n]{1,40}",
            px in -600.0..600.0f64, py in -600.0..600.0f64,
        ) {
            let note = MTextEntity::new(p(x, y), width, height, rotation, content.clone());
            let bounds = note.bounds();
            let loose = Aabb::from_points(
                bounds.min() - Vec2::new(1e-6, 1e-6), bounds.max() + Vec2::new(1e-6, 1e-6));
            for point in note.strokes().iter().flatten() {
                prop_assert!(loose.contains_point(*point), "{point:?} outside {bounds:?}");
            }
            let probe = p(px, py);
            prop_assert_eq!(note.distance_to(probe) == 0.0, note.frame().contains(probe));
            let mirrored = note.transformed(&Transform::mirror(p(0.0, 0.0), p(1.0, 1.0)).unwrap());
            prop_assert!(mirrored.rotation.cos() >= -1e-6, "never upside down");
            prop_assert_eq!(mirrored.lines(), note.lines());
        }
    }
}
