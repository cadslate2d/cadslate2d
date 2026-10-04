//! Packed geometry shared by both render lifetimes.
//!
//! The layout is the M0 starting point from ADR-0003 and stays PROVISIONAL
//! until the transport measurement: one *segment instance* is four `f32`s
//! (`x0, y0, x1, y1`), each an offset from the frame's `f64` origin. Stroke
//! width is applied in the vertex shader, so the data is independent of zoom.

use cad_core::Point2;
use cad_document::Entity;

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
}

/// A contiguous run of segment instances drawn with one style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawBatch {
    pub style: StyleRole,
    /// Index of the first segment instance.
    pub first: u32,
    /// Number of segment instances.
    pub count: u32,
}

/// Geometry and batches ready to cross the WASM boundary in bulk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackedFrame {
    origin: Point2,
    segments: Vec<f32>,
    batches: Vec<DrawBatch>,
}

impl PackedFrame {
    pub const FLOATS_PER_SEGMENT: usize = 4;
    pub const WORDS_PER_BATCH: usize = 3;

    /// Empties the frame and sets a new origin. Buffer capacity is kept, so
    /// rebuilding a frame of similar size does not allocate.
    pub fn reset(&mut self, origin: Point2) {
        self.origin = origin;
        self.segments.clear();
        self.batches.clear();
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

    pub fn batches(&self) -> &[DrawBatch] {
        &self.batches
    }

    /// Batches flattened to `style, first, count` words for transfer.
    pub fn write_batch_words(&self, out: &mut Vec<u32>) {
        out.clear();
        for batch in &self.batches {
            out.extend([batch.style as u32, batch.first, batch.count]);
        }
    }

    /// Appends one segment. Consecutive segments of the same style share a batch.
    pub fn push_segment(&mut self, style: StyleRole, start: Point2, end: Point2) {
        let first = self.segment_count();
        let a = start - self.origin;
        let b = end - self.origin;
        self.segments
            .extend([a.x as f32, a.y as f32, b.x as f32, b.y as f32]);

        match self.batches.last_mut() {
            Some(batch) if batch.style == style => batch.count += 1,
            _ => self.batches.push(DrawBatch {
                style,
                first,
                count: 1,
            }),
        }
    }

    /// Appends the stroke geometry of one entity. The single place that knows
    /// how each entity kind becomes segments, shared by scene and overlay.
    pub fn push_entity(&mut self, style: StyleRole, entity: &Entity) {
        match entity {
            Entity::Line(line) => self.push_segment(style, line.start, line.end),
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
            &[
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
        let mut words = vec![99];
        frame.write_batch_words(&mut words);
        assert_eq!(words, vec![1, 0, 1, 3, 1, 1]);
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
    fn a_line_entity_becomes_one_segment() {
        let mut frame = PackedFrame::default();
        let line = Entity::Line(LineEntity::new(p(0.0, 0.0), p(3.0, 4.0)));
        frame.push_entity(StyleRole::Entity, &line);
        assert_eq!(frame.segments(), &[0.0, 0.0, 3.0, 4.0]);
    }
}
