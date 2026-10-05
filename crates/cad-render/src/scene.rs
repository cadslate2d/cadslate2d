//! The committed render scene (DESIGN §12.2–12.3).

use cad_core::Point2;
use cad_document::{Drawing, Revision};

use crate::{PackedFrame, StyleRole};

/// Stable geometry derived from the canonical document.
///
/// A scene is a function of the drawing alone — there is no view parameter —
/// so pan, zoom, and pointer movement can never invalidate it (ADR-0003).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderScene {
    revision: Revision,
    frame: PackedFrame,
}

impl RenderScene {
    /// The document revision this scene was built from. TypeScript re-uploads
    /// committed GPU buffers only when this value changes.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn frame(&self) -> &PackedFrame {
        &self.frame
    }

    fn build_into(frame: &mut PackedFrame, drawing: &Drawing) {
        // Centre the origin on the drawing so f32 offsets keep the most
        // precision where the geometry actually is.
        let origin = drawing
            .extents()
            .map_or(Point2::ORIGIN, |extents| extents.center());
        frame.reset(origin);
        // Objects on a layer that is off are not drawn.
        for (_, entity) in drawing.shown() {
            frame.push_entity(StyleRole::Entity, entity);
        }
    }
}

/// Holds the current scene and rebuilds it only when the drawing's revision
/// has changed since the last request.
#[derive(Debug, Default)]
pub struct SceneCache {
    scene: Option<RenderScene>,
    builds: u64,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The scene for `drawing`, rebuilt if and only if the drawing changed.
    pub fn scene(&mut self, drawing: &Drawing) -> &RenderScene {
        let revision = drawing.revision();
        let stale = self
            .scene
            .as_ref()
            .is_none_or(|scene| scene.revision != revision);
        if stale {
            // Reuse the previous frame's buffers to avoid reallocating.
            let mut frame = self
                .scene
                .take()
                .map(|scene| scene.frame)
                .unwrap_or_default();
            RenderScene::build_into(&mut frame, drawing);
            self.scene = Some(RenderScene { revision, frame });
            self.builds += 1;
        }
        self.scene.as_ref().expect("scene was just built")
    }

    /// How many times a scene has been built. Tests use this to prove that
    /// pointer activity does not regenerate committed geometry.
    pub fn build_count(&self) -> u64 {
        self.builds
    }
}

#[cfg(test)]
mod tests {
    use cad_document::LineEntity;

    use super::*;
    use crate::DrawBatch;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
        LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    fn add(drawing: &mut Drawing, entity: LineEntity) {
        let mut tx = drawing.transaction();
        tx.insert(entity);
        tx.commit();
    }

    fn sample() -> Drawing {
        let mut drawing = Drawing::new();
        add(&mut drawing, line(0.0, 0.0, 100.0, 0.0));
        add(&mut drawing, line(100.0, 0.0, 100.0, 50.0));
        add(&mut drawing, line(100.0, 50.0, 0.0, 0.0));
        drawing
    }

    #[test]
    fn an_empty_drawing_yields_an_empty_scene() {
        let mut cache = SceneCache::new();
        let drawing = Drawing::new();
        let scene = cache.scene(&drawing);
        assert_eq!(scene.frame().segment_count(), 0);
        assert!(scene.frame().batches().is_empty());
        assert_eq!(scene.revision(), drawing.revision());
    }

    #[test]
    fn every_line_becomes_one_segment_in_one_entity_batch() {
        let drawing = sample();
        let mut cache = SceneCache::new();
        let frame = cache.scene(&drawing).frame();
        assert_eq!(frame.segment_count(), 3);
        assert_eq!(
            frame.batches(),
            &[DrawBatch {
                style: StyleRole::Entity,
                first: 0,
                count: 3
            }]
        );
    }

    #[test]
    fn geometry_is_relative_to_the_centre_of_the_extents() {
        let drawing = sample();
        let mut cache = SceneCache::new();
        let frame = cache.scene(&drawing).frame();
        assert_eq!(frame.origin(), Point2::new(50.0, 25.0));
        assert_eq!(&frame.segments()[..4], &[-50.0, -25.0, 50.0, -25.0]);
    }

    #[test]
    fn the_same_drawing_always_yields_the_same_scene() {
        let (a, b) = (sample(), sample());
        let mut cache_a = SceneCache::new();
        let mut cache_b = SceneCache::new();
        // Two independently built but identical drawings: identical bytes.
        assert_eq!(cache_a.scene(&a).frame(), cache_b.scene(&b).frame());
        // And building again from the same drawing changes nothing.
        let first = cache_a.scene(&a).clone();
        assert_eq!(SceneCache::new().scene(&a), &first);
    }

    #[test]
    fn the_scene_is_not_rebuilt_without_a_document_mutation() {
        let drawing = sample();
        let mut cache = SceneCache::new();
        for _ in 0..1000 {
            cache.scene(&drawing);
        }
        assert_eq!(cache.build_count(), 1);
    }

    #[test]
    fn commit_undo_and_redo_each_produce_a_new_scene() {
        let mut drawing = sample();
        let mut cache = SceneCache::new();
        cache.scene(&drawing);

        add(&mut drawing, line(0.0, 0.0, 0.0, 10.0));
        assert_eq!(cache.scene(&drawing).frame().segment_count(), 4);
        drawing.undo();
        assert_eq!(cache.scene(&drawing).frame().segment_count(), 3);
        drawing.redo();
        assert_eq!(cache.scene(&drawing).frame().segment_count(), 4);

        assert_eq!(cache.build_count(), 4);
        assert_eq!(cache.scene(&drawing).revision(), drawing.revision());
    }

    #[test]
    fn replacing_the_drawing_invalidates_the_cache() {
        let mut cache = SceneCache::new();
        cache.scene(&sample());
        let other = Drawing::new();
        assert_eq!(cache.scene(&other).frame().segment_count(), 0);
        assert_eq!(cache.build_count(), 2);
    }

    #[test]
    fn batch_ranges_stay_within_the_geometry_buffer() {
        let drawing = sample();
        let mut cache = SceneCache::new();
        let frame = cache.scene(&drawing).frame();
        assert_eq!(
            frame.segments().len(),
            frame.segment_count() as usize * PackedFrame::FLOATS_PER_SEGMENT
        );
        for batch in frame.batches() {
            assert!(batch.first + batch.count <= frame.segment_count());
        }
    }
}
