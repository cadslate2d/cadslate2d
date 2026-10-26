//! Leaders (#205; ADR-0020): a line of two or more vertices with an
//! arrowhead at its first and, at its last, a landing and a note. Like a
//! dimension, a leader stores what defines it — its vertices, its words,
//! its style — and is laid out afresh from the style whenever it is
//! drawn, picked, or bounded: the arrowhead's size, the text's height,
//! and the gap are the style's, so a style change redraws every leader
//! as one row changes.

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Arrowhead, Segment, Transform, normalized};

use crate::dim_style::{DimStyle, DimStyleId};
use crate::text::{Justification, TextEntity};

/// A leader: the points it runs through, from the arrowhead's tip to
/// the note, the note's words if any, and the style it is drawn by.
#[derive(Clone, Debug, PartialEq)]
pub struct LeaderEntity {
    pub vertices: Vec<Point2>,
    pub text: Option<String>,
    pub style: DimStyleId,
}

/// Why a leader cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderError {
    /// A leader runs through at least two points.
    TooFewPoints,
    /// A vertex that is not a finite point.
    NotFinite,
}

impl std::fmt::Display for LeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewPoints => write!(f, "a leader needs at least two points"),
            Self::NotFinite => write!(f, "a leader's points must be finite"),
        }
    }
}

impl std::error::Error for LeaderError {}

/// A leader drawn: what the renderer draws, picking feels, and the
/// bounds hold. Built by [`LeaderEntity::layout`] from a style.
#[derive(Clone, Debug, PartialEq)]
pub struct LeaderLayout {
    /// The leader's segments, then the landing before the note.
    pub lines: Vec<Segment>,
    /// At the first vertex, pointing back along the first segment.
    pub arrowhead: Option<Arrowhead>,
    /// The note, placed past the landing.
    pub text: Option<TextEntity>,
}

impl LeaderLayout {
    /// Every straight piece: the lines and the arrowhead's outline.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.lines
            .iter()
            .copied()
            .chain(self.arrowhead.iter().flat_map(|arrow| arrow.segments()))
    }

    pub fn bounds(&self) -> Aabb {
        let mut bounds = self
            .segments()
            .map(|segment| segment.bounds())
            .reduce(|a, b| a.union(&b))
            .unwrap_or_else(|| Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN));
        if let Some(text) = &self.text {
            bounds = bounds.union(&text.bounds());
        }
        bounds
    }

    /// The distance to the nearest line, the arrowhead, or the note's box.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.segments()
            .map(|segment| segment.distance_to_point(point))
            .fold(
                self.text
                    .as_ref()
                    .map_or(f64::INFINITY, |text| text.distance_to(point)),
                f64::min,
            )
    }
}

impl LeaderEntity {
    /// A leader through `vertices`, with no note, in the standard style.
    pub fn new(vertices: Vec<Point2>) -> Self {
        Self {
            vertices,
            text: None,
            style: DimStyleId::FIRST,
        }
    }

    #[must_use]
    pub fn saying(self, text: Option<String>) -> Self {
        Self {
            text: text.filter(|text| !text.trim().is_empty()),
            ..self
        }
    }

    #[must_use]
    pub fn in_style(self, style: DimStyleId) -> Self {
        Self { style, ..self }
    }

    pub fn check(&self) -> Result<(), LeaderError> {
        if self.vertices.len() < 2 {
            return Err(LeaderError::TooFewPoints);
        }
        if self
            .vertices
            .iter()
            .any(|vertex| !(vertex.x.is_finite() && vertex.y.is_finite()))
        {
            return Err(LeaderError::NotFinite);
        }
        Ok(())
    }

    /// This leader with vertex `index` at `to`.
    pub fn with_vertex(&self, index: usize, to: Point2) -> Option<Self> {
        (index < self.vertices.len()).then(|| {
            let mut vertices = self.vertices.clone();
            vertices[index] = to;
            Self {
                vertices,
                ..self.clone()
            }
        })
    }

    /// Which way the note goes from the last vertex: along the last
    /// segment's X direction, right when it has none.
    fn reads_right(&self) -> bool {
        let n = self.vertices.len();
        n < 2 || self.vertices[n - 1].x >= self.vertices[n - 2].x
    }

    /// The leader laid out by `style`: its segments, an arrowhead the
    /// style's size at its first vertex pointing back along the first
    /// segment, and — when it has a note — a landing the arrowhead's
    /// length from the last vertex, left or right as the leader ends,
    /// and the note after the style's gap, its baseline the text's half
    /// height below the landing.
    pub fn layout(&self, style: &DimStyle) -> LeaderLayout {
        // Every size at the style's scale (#327).
        let style = &style.scaled();
        let mut lines: Vec<Segment> = self
            .vertices
            .windows(2)
            .map(|pair| Segment::new(pair[0], pair[1]))
            .collect();
        let arrowhead = self.vertices.get(1).and_then(|second| {
            let tip = self.vertices[0];
            normalized(tip - *second)
                .map(|direction| Arrowhead::new(tip, direction, style.arrow_size))
        });
        let text = self.text.as_ref().and_then(|words| {
            let last = *self.vertices.last()?;
            let right = self.reads_right();
            let along = Vec2::new(if right { 1.0 } else { -1.0 }, 0.0);
            let landing = last + along * style.arrow_size;
            lines.push(Segment::new(last, landing));
            let insertion =
                landing + along * style.text_gap - Vec2::new(0.0, style.text_height / 2.0);
            Some(
                TextEntity::new(insertion, style.text_height, 0.0, words.clone()).justified(
                    if right {
                        Justification::Left
                    } else {
                        Justification::Right
                    },
                ),
            )
        });
        LeaderLayout {
            lines,
            arrowhead,
            text,
        }
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            vertices: self
                .vertices
                .iter()
                .map(|vertex| transform.apply(*vertex))
                .collect(),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn style() -> DimStyle {
        DimStyle {
            text_height: 2.5,
            arrow_size: 3.0,
            text_gap: 1.0,
            ..DimStyle::named("Standard")
        }
    }

    #[test]
    fn a_leader_is_laid_out_from_its_style_with_the_note_past_a_landing() {
        let leader = LeaderEntity::new(vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 10.0)])
            .saying(Some("Note".to_owned()));
        let layout = leader.layout(&style());
        assert_eq!(layout.lines.len(), 3, "two segments and the landing");
        assert_eq!(layout.lines[2], Segment::new(p(20.0, 10.0), p(23.0, 10.0)));
        let arrow = layout.arrowhead.unwrap();
        assert_eq!(arrow.tip, p(0.0, 0.0));
        assert!((arrow.length() - 3.0).abs() < 1e-9);
        let text = layout.text.clone().unwrap();
        assert_eq!(text.insertion, p(24.0, 10.0 - 1.25));
        assert_eq!(text.height, 2.5);
        assert_eq!(text.justification, Justification::Left);
        assert_eq!(text.content, "Note");
        assert!(layout.bounds().contains_point(p(0.0, 0.0)));
        assert!(layout.distance_to(p(24.5, 10.0)) < 1e-9, "on the note");

        // Ending leftwards, the note reads back from the landing.
        let left = LeaderEntity::new(vec![p(20.0, 0.0), p(0.0, 10.0)]).saying(Some("L".to_owned()));
        let layout = left.layout(&style());
        assert_eq!(layout.lines[1], Segment::new(p(0.0, 10.0), p(-3.0, 10.0)));
        assert_eq!(layout.text.unwrap().justification, Justification::Right);

        // No note: no landing, no text; a blank note is none.
        let bare = LeaderEntity::new(vec![p(0.0, 0.0), p(5.0, 5.0)]).saying(Some("  ".to_owned()));
        assert_eq!(bare.text, None);
        assert!(bare.layout(&style()).text.is_none());
        assert_eq!(bare.layout(&style()).lines.len(), 1);
        assert_eq!(
            LeaderEntity::new(vec![p(0.0, 0.0)]).check(),
            Err(LeaderError::TooFewPoints)
        );
        assert_eq!(
            LeaderEntity::new(vec![p(0.0, 0.0), p(f64::NAN, 0.0)]).check(),
            Err(LeaderError::NotFinite)
        );
    }

    proptest! {
        /// The arrowhead's tip is the first vertex and it points back along
        /// the first segment; every line of the layout is within the bounds;
        /// the note's insertion is the gap past the landing; and the layout
        /// of a moved leader is the moved layout.
        #[test]
        fn the_layout_follows_the_vertices_and_the_style(
            points in prop::collection::vec((-100.0..100.0_f64, -100.0..100.0_f64), 2..6),
            arrow in 0.5..10.0_f64, height in 0.5..10.0_f64, gap in 0.0..5.0_f64,
            dx in -50.0..50.0_f64, dy in -50.0..50.0_f64,
        ) {
            let vertices: Vec<Point2> = points.iter().map(|(x, y)| p(*x, *y)).collect();
            prop_assume!(vertices.windows(2).all(|w| w[0].distance_to(w[1]) > 1e-3));
            let style = DimStyle { text_height: height, arrow_size: arrow, text_gap: gap, ..DimStyle::named("S") };
            let leader = LeaderEntity::new(vertices.clone()).saying(Some("Ab".to_owned()));
            let layout = leader.layout(&style);
            let arrowhead = layout.arrowhead.unwrap();
            prop_assert_eq!(arrowhead.tip, vertices[0]);
            let back = arrowhead.base[0] + (arrowhead.base[1] - arrowhead.base[0]) * 0.5;
            let along = normalized(vertices[1] - vertices[0]).unwrap();
            prop_assert!((back - vertices[0]).dot(along) > 0.0, "the base lies down the first segment");
            let bounds = layout.bounds().expanded(1e-9);
            for segment in layout.segments() {
                prop_assert!(bounds.contains_point(segment.start) && bounds.contains_point(segment.end));
            }
            let last = *vertices.last().unwrap();
            let text = layout.text.unwrap();
            let right = last.x >= vertices[vertices.len() - 2].x;
            let expected = last + Vec2::new(if right { arrow + gap } else { -(arrow + gap) }, -height / 2.0);
            prop_assert!(text.insertion.distance_to(expected) < 1e-9);
            let moved = leader.transformed(&Transform::translation(Vec2::new(dx, dy)));
            let moved_layout = moved.layout(&style);
            prop_assert!(moved_layout.text.unwrap().insertion.distance_to(expected + Vec2::new(dx, dy)) < 1e-9);
            prop_assert_eq!(moved_layout.lines.len(), layout.lines.len());
        }
    }
}
