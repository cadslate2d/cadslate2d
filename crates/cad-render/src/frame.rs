//! Packed geometry shared by both render lifetimes.
//!
//! The layout is the M0 starting point from ADR-0003 and stays PROVISIONAL
//! until the transport measurement: one *segment instance* is four `f32`s
//! (`x0, y0, x1, y1`), each an offset from the frame's `f64` origin. Stroke
//! width is applied in the vertex shader, so the data is independent of zoom.
//!
//! Curves are *arc instances* (ADR-0009): five `f32`s, `cx, cy, radius,
//! start, sweep`, evaluated exactly in the shader. They travel in their own
//! buffer with their own batches.
//!
//! All buffers are stored exactly as they cross the WASM boundary, so
//! handing a frame to JavaScript needs no conversion and no allocation.

use cad_core::Point2;
use cad_document::Entity;
use cad_geometry::{Arc, Primitive};

/// What a batch of geometry *is*. TypeScript resolves each role to a colour
/// and lineweight from the theme, so Rust never hard-codes theme colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum StyleRole {
    /// Committed drawing geometry.
    Entity = 0,
    /// Highlight of selected entities.
    Selection = 1,
    /// Transient command preview, such as a rubber-band line.
    Preview = 2,
    /// The cursor crosshair.
    Crosshair = 3,
    /// The rectangle of a window selection (entities entirely inside).
    SelectionWindow = 4,
    /// The rectangle of a crossing selection (entities touching it).
    SelectionCrossing = 5,
    /// The marker of the object snap under the cursor.
    SnapMarker = 6,
    /// The direction polar tracking has locked onto.
    Tracking = 7,
    /// Ordinary lines of the reference grid.
    GridMinor = 8,
    /// Every fifth line of the reference grid.
    GridMajor = 9,
    /// A grip of a selected entity.
    Grip = 10,
    /// The grip under the cursor, or the one being dragged.
    GripHot = 11,
}

impl StyleRole {
    const ALL: [Self; 12] = [
        Self::Entity,
        Self::Selection,
        Self::Preview,
        Self::Crosshair,
        Self::SelectionWindow,
        Self::SelectionCrossing,
        Self::SnapMarker,
        Self::Tracking,
        Self::GridMinor,
        Self::GridMajor,
        Self::Grip,
        Self::GripHot,
    ];

    fn from_word(word: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|role| *role as u32 == word)
    }
}

/// A contiguous run of instances (segments or arcs) drawn with one style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawBatch {
    pub style: StyleRole,
    /// Index of the first instance.
    pub first: u32,
    /// Number of instances.
    pub count: u32,
}

/// Geometry and batches ready to cross the WASM boundary in bulk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackedFrame {
    origin: Point2,
    segments: Vec<f32>,
    /// `style, first, count` per segment batch.
    batch_words: Vec<u32>,
    /// `cx, cy, radius, start, sweep` per arc instance.
    arcs: Vec<f32>,
    /// `style, first, count` per arc batch.
    arc_batch_words: Vec<u32>,
}

/// Appends one instance to a batch list, extending the last batch when the
/// style is unchanged.
fn extend_batches(words: &mut Vec<u32>, style: StyleRole, index: u32) {
    match words.rchunks_exact_mut(PackedFrame::WORDS_PER_BATCH).next() {
        Some(last) if last[0] == style as u32 => last[2] += 1,
        _ => words.extend([style as u32, index, 1]),
    }
}

fn decode_batches(words: &[u32]) -> Vec<DrawBatch> {
    let (batches, _) = words.as_chunks::<{ PackedFrame::WORDS_PER_BATCH }>();
    batches
        .iter()
        .map(|&[style, first, count]| DrawBatch {
            style: StyleRole::from_word(style).expect("frames only contain known style roles"),
            first,
            count,
        })
        .collect()
}

impl PackedFrame {
    pub const FLOATS_PER_SEGMENT: usize = 4;
    pub const WORDS_PER_BATCH: usize = 3;
    pub const FLOATS_PER_ARC: usize = 5;

    /// Empties the frame and sets a new origin. Buffer capacity is kept, so
    /// rebuilding a frame of similar size does not allocate.
    pub fn reset(&mut self, origin: Point2) {
        self.origin = origin;
        self.segments.clear();
        self.batch_words.clear();
        self.arcs.clear();
        self.arc_batch_words.clear();
    }

    /// The `f64` world position that segment coordinates are relative to.
    pub fn origin(&self) -> Point2 {
        self.origin
    }

    /// Flat `x0, y0, x1, y1` per segment instance.
    pub fn segments(&self) -> &[f32] {
        &self.segments
    }

    pub fn segment_count(&self) -> u32 {
        (self.segments.len() / Self::FLOATS_PER_SEGMENT) as u32
    }

    /// Flat `style, first, count` per batch.
    pub fn batch_words(&self) -> &[u32] {
        &self.batch_words
    }

    /// The segment batches, decoded.
    pub fn batches(&self) -> Vec<DrawBatch> {
        decode_batches(&self.batch_words)
    }

    /// Flat `cx, cy, radius, start, sweep` per arc instance.
    pub fn arcs(&self) -> &[f32] {
        &self.arcs
    }

    pub fn arc_count(&self) -> u32 {
        (self.arcs.len() / Self::FLOATS_PER_ARC) as u32
    }

    /// Flat `style, first, count` per arc batch.
    pub fn arc_batch_words(&self) -> &[u32] {
        &self.arc_batch_words
    }

    /// The arc batches, decoded.
    pub fn arc_batches(&self) -> Vec<DrawBatch> {
        decode_batches(&self.arc_batch_words)
    }

    /// Appends one segment. Consecutive segments of the same style share a batch.
    pub fn push_segment(&mut self, style: StyleRole, start: Point2, end: Point2) {
        let first = self.segment_count();
        let a = start - self.origin;
        let b = end - self.origin;
        self.segments
            .extend([a.x as f32, a.y as f32, b.x as f32, b.y as f32]);

        extend_batches(&mut self.batch_words, style, first);
    }

    /// Appends one arc instance. A full circle is an arc with a sweep of 2π.
    pub fn push_arc(&mut self, style: StyleRole, arc: &Arc) {
        let index = self.arc_count();
        let center = arc.center - self.origin;
        self.arcs.extend([
            center.x as f32,
            center.y as f32,
            arc.radius as f32,
            arc.start as f32,
            arc.sweep as f32,
        ]);
        extend_batches(&mut self.arc_batch_words, style, index);
    }

    /// Appends the stroke geometry of one entity. The single place that knows
    /// how each entity kind becomes segments, shared by scene and overlay.
    pub fn push_entity(&mut self, style: StyleRole, entity: &Entity) {
        match entity {
            Entity::Line(line) => self.push_segment(style, line.start, line.end),
            Entity::Circle(circle) => {
                self.push_arc(
                    style,
                    &Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU),
                );
            }
            Entity::Arc(arc) => self.push_arc(style, &arc.arc()),
            // Round caps on every segment meet at the vertices, so joints have
            // neither gaps nor spikes at any angle. Arc segments are drawn
            // as arcs, exactly like arc entities (ADR-0009).
            Entity::Polyline(polyline) => {
                for primitive in polyline.primitives() {
                    match primitive {
                        Primitive::Segment(segment) => {
                            self.push_segment(style, segment.start, segment.end);
                        }
                        Primitive::Arc(arc) => self.push_arc(style, &arc),
                        Primitive::Circle(circle) => self.push_arc(
                            style,
                            &Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU),
                        ),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cad_document::LineEntity;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn segments_are_stored_relative_to_the_origin() {
        let mut frame = PackedFrame::default();
        frame.reset(p(1000.0, 2000.0));
        frame.push_segment(StyleRole::Entity, p(1001.0, 2000.5), p(990.0, 2010.0));
        assert_eq!(frame.segments(), &[1.0, 0.5, -10.0, 10.0]);
        assert_eq!(frame.segment_count(), 1);
    }

    #[test]
    fn consecutive_segments_of_one_style_share_a_batch() {
        let mut frame = PackedFrame::default();
        frame.push_segment(StyleRole::Entity, p(0.0, 0.0), p(1.0, 0.0));
        frame.push_segment(StyleRole::Entity, p(1.0, 0.0), p(2.0, 0.0));
        frame.push_segment(StyleRole::Preview, p(2.0, 0.0), p(3.0, 0.0));
        frame.push_segment(StyleRole::Entity, p(3.0, 0.0), p(4.0, 0.0));

        assert_eq!(
            frame.batches(),
            vec![
                DrawBatch {
                    style: StyleRole::Entity,
                    first: 0,
                    count: 2
                },
                DrawBatch {
                    style: StyleRole::Preview,
                    first: 2,
                    count: 1
                },
                DrawBatch {
                    style: StyleRole::Entity,
                    first: 3,
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn batch_words_are_style_first_count_triples() {
        let mut frame = PackedFrame::default();
        frame.push_segment(StyleRole::Selection, p(0.0, 0.0), p(1.0, 0.0));
        frame.push_segment(StyleRole::Crosshair, p(0.0, 0.0), p(1.0, 0.0));
        assert_eq!(frame.batch_words(), &[1, 0, 1, 3, 1, 1]);
    }

    #[test]
    fn every_style_role_survives_the_word_encoding() {
        for role in StyleRole::ALL {
            assert_eq!(StyleRole::from_word(role as u32), Some(role));
        }
        assert_eq!(StyleRole::from_word(99), None);
    }

    #[test]
    fn reset_empties_the_frame_but_keeps_capacity() {
        let mut frame = PackedFrame::default();
        for i in 0..100 {
            frame.push_segment(StyleRole::Entity, p(0.0, 0.0), p(f64::from(i), 0.0));
        }
        let capacity = frame.segments.capacity();
        frame.reset(p(5.0, 5.0));

        assert_eq!(frame.segment_count(), 0);
        assert!(frame.batches().is_empty());
        assert_eq!(frame.origin(), p(5.0, 5.0));
        assert_eq!(frame.segments.capacity(), capacity);
    }

    #[test]
    fn circles_and_arcs_become_arc_instances_relative_to_the_origin() {
        use std::f64::consts::{FRAC_PI_2, PI, TAU};

        use cad_document::{ArcEntity, CircleEntity};

        let mut frame = PackedFrame::default();
        frame.reset(p(100.0, 100.0));
        frame.push_entity(
            StyleRole::Entity,
            &CircleEntity::new(p(110.0, 95.0), 4.0).into(),
        );
        frame.push_entity(
            StyleRole::Entity,
            &ArcEntity::new(p(100.0, 100.0), 2.0, FRAC_PI_2, PI).into(),
        );
        frame.push_entity(
            StyleRole::Preview,
            &CircleEntity::new(p(0.0, 0.0), 1.0).into(),
        );

        assert_eq!(frame.arc_count(), 3);
        assert_eq!(&frame.arcs()[..5], &[10.0, -5.0, 4.0, 0.0, TAU as f32]);
        assert_eq!(
            &frame.arcs()[5..10],
            &[0.0, 0.0, 2.0, FRAC_PI_2 as f32, FRAC_PI_2 as f32]
        );
        assert_eq!(
            frame.arc_batches(),
            vec![
                DrawBatch {
                    style: StyleRole::Entity,
                    first: 0,
                    count: 2
                },
                DrawBatch {
                    style: StyleRole::Preview,
                    first: 2,
                    count: 1
                },
            ]
        );
        // Arcs and segments are independent buffers.
        assert_eq!(frame.segment_count(), 0);
        assert!(frame.batches().is_empty());

        frame.reset(p(0.0, 0.0));
        assert_eq!(frame.arc_count(), 0);
        assert!(frame.arc_batch_words().is_empty());
    }

    /// #114: arc segments of a polyline are drawn as arcs, like arc entities.
    #[test]
    fn a_bulged_polyline_becomes_segments_and_arcs() {
        use cad_document::PolylineEntity;
        let slot = PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        );
        let mut frame = PackedFrame::default();
        frame.push_entity(StyleRole::Entity, &slot.into());

        assert_eq!(
            frame.segments(),
            &[0.0, 0.0, 10.0, 0.0, 10.0, 4.0, 0.0, 4.0]
        );
        assert_eq!(frame.arc_count(), 2);
        // Centre and radius of the right end, then of the left.
        assert_eq!(&frame.arcs()[..3], &[10.0, 2.0, 2.0]);
        assert_eq!(&frame.arcs()[5..8], &[0.0, 2.0, 2.0]);
        assert_eq!(frame.batches().len(), 1);
        assert_eq!(frame.arc_batches().len(), 1);
    }

    #[test]
    fn a_polyline_becomes_one_segment_per_edge_in_a_single_batch() {
        use cad_document::PolylineEntity;
        let corners = vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0)];
        let mut frame = PackedFrame::default();
        frame.push_entity(
            StyleRole::Entity,
            &PolylineEntity::new(corners.clone(), false).into(),
        );
        assert_eq!(frame.segments(), &[0.0, 0.0, 4.0, 0.0, 4.0, 0.0, 4.0, 3.0]);

        frame.reset(p(0.0, 0.0));
        frame.push_entity(
            StyleRole::Entity,
            &PolylineEntity::new(corners, true).into(),
        );
        assert_eq!(frame.segment_count(), 3);
        assert_eq!(
            &frame.segments()[8..],
            &[4.0, 3.0, 0.0, 0.0],
            "the closing edge"
        );
        assert_eq!(
            frame.batches(),
            vec![DrawBatch {
                style: StyleRole::Entity,
                first: 0,
                count: 3
            }]
        );
    }

    #[test]
    fn a_line_entity_becomes_one_segment() {
        let mut frame = PackedFrame::default();
        let line = Entity::Line(LineEntity::new(p(0.0, 0.0), p(3.0, 4.0)));
        frame.push_entity(StyleRole::Entity, &line);
        assert_eq!(frame.segments(), &[0.0, 0.0, 3.0, 4.0]);
    }
}
