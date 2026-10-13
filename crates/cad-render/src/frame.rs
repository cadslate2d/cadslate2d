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
//! Beside each instance travels where along its entity's path it begins
//! (ADR-0015), so a linetype pattern can run on through the vertices of a
//! polyline instead of starting again at each: one `f32` per segment, and
//! for an arc that distance at its counter-clockwise start plus the
//! direction the path runs round it. Only the committed scene draws them;
//! the overlay is always continuous.
//!
//! A filled region is *triangle instances* (ADR-0021): six `f32`s, the
//! three corners, each an offset from the origin like a segment's ends.
//! They travel in their own buffer with their own batches, and carry no
//! path distance: a fill has no linetype.
//!
//! All buffers are stored exactly as they cross the WASM boundary, so
//! handing a frame to JavaScript needs no conversion and no allocation.

use cad_core::Point2;
use cad_document::{Entity, HatchStyle, PolylineEntity, Properties, Tables};
use cad_geometry::{Arc, Primitive, bulge_primitive, polyline_spans};

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
    /// The object a click would pick, while a command asks for one.
    Hover = 12,
    /// Geometry an edit would remove.
    Removal = 13,
}

impl StyleRole {
    const ALL: [Self; 14] = [
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
        Self::Hover,
        Self::Removal,
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
    /// Per segment instance: the distance along its entity's path at its
    /// start, in drawing units.
    segment_distances: Vec<f32>,
    /// Per arc instance: that distance at the arc's counter-clockwise
    /// start, then `1` when the path runs counter-clockwise round it and
    /// `-1` when clockwise.
    arc_distances: Vec<f32>,
    /// `x0, y0, x1, y1, x2, y2` per triangle instance.
    triangles: Vec<f32>,
    /// `style, first, count` per triangle batch.
    triangle_batch_words: Vec<u32>,
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
    /// Per arc in [`Self::arc_distances`]: the distance and the direction.
    pub const FLOATS_PER_ARC_DISTANCE: usize = 2;
    pub const FLOATS_PER_TRIANGLE: usize = 6;

    /// Empties the frame and sets a new origin. Buffer capacity is kept, so
    /// rebuilding a frame of similar size does not allocate.
    pub fn reset(&mut self, origin: Point2) {
        self.origin = origin;
        self.segments.clear();
        self.batch_words.clear();
        self.arcs.clear();
        self.arc_batch_words.clear();
        self.segment_distances.clear();
        self.arc_distances.clear();
        self.triangles.clear();
        self.triangle_batch_words.clear();
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

    /// Flat `x0, y0, x1, y1, x2, y2` per triangle instance.
    pub fn triangles(&self) -> &[f32] {
        &self.triangles
    }

    pub fn triangle_count(&self) -> u32 {
        (self.triangles.len() / Self::FLOATS_PER_TRIANGLE) as u32
    }

    /// Flat `style, first, count` per triangle batch.
    pub fn triangle_batch_words(&self) -> &[u32] {
        &self.triangle_batch_words
    }

    /// The triangle batches, decoded.
    pub fn triangle_batches(&self) -> Vec<DrawBatch> {
        decode_batches(&self.triangle_batch_words)
    }

    /// Per segment instance, the distance along its entity's path at its
    /// start, in drawing units (ADR-0015).
    pub fn segment_distances(&self) -> &[f32] {
        &self.segment_distances
    }

    /// Per arc instance, [`Self::FLOATS_PER_ARC_DISTANCE`] floats: the
    /// distance along its entity's path at the arc's counter-clockwise
    /// start, and `1` or `-1` for the direction the path runs round it.
    pub fn arc_distances(&self) -> &[f32] {
        &self.arc_distances
    }

    // ── what a chunked scene writes directly (ADR-0022) ──

    pub(crate) fn segments_mut(&mut self) -> &mut Vec<f32> {
        &mut self.segments
    }

    pub(crate) fn segment_distances_mut(&mut self) -> &mut Vec<f32> {
        &mut self.segment_distances
    }

    pub(crate) fn arcs_mut(&mut self) -> &mut Vec<f32> {
        &mut self.arcs
    }

    pub(crate) fn arc_distances_mut(&mut self) -> &mut Vec<f32> {
        &mut self.arc_distances
    }

    pub(crate) fn triangles_mut(&mut self) -> &mut Vec<f32> {
        &mut self.triangles
    }

    /// Replaces every batch with one of `style` over all the instances of
    /// each kind there are.
    pub(crate) fn batch_everything(&mut self, style: StyleRole) {
        let one = |count: u32| {
            if count == 0 {
                Vec::new()
            } else {
                vec![style as u32, 0, count]
            }
        };
        self.batch_words = one(self.segment_count());
        self.arc_batch_words = one(self.arc_count());
        self.triangle_batch_words = one(self.triangle_count());
    }

    /// Appends one segment at the start of its path. Consecutive segments
    /// of the same style share a batch.
    pub fn push_segment(&mut self, style: StyleRole, start: Point2, end: Point2) {
        self.push_segment_along(style, start, end, 0.0);
    }

    /// Appends one segment that begins `distance` along its entity's path.
    pub fn push_segment_along(
        &mut self,
        style: StyleRole,
        start: Point2,
        end: Point2,
        distance: f64,
    ) {
        let first = self.segment_count();
        let a = start - self.origin;
        let b = end - self.origin;
        self.segments
            .extend([a.x as f32, a.y as f32, b.x as f32, b.y as f32]);
        self.segment_distances.push(distance as f32);
        extend_batches(&mut self.batch_words, style, first);
    }

    /// Appends one filled triangle. Consecutive triangles of the same style
    /// share a batch.
    pub fn push_triangle(&mut self, style: StyleRole, corners: [Point2; 3]) {
        let first = self.triangle_count();
        for corner in corners {
            let at = corner - self.origin;
            self.triangles.extend([at.x as f32, at.y as f32]);
        }
        extend_batches(&mut self.triangle_batch_words, style, first);
    }

    /// Appends one arc instance at the start of its path, run
    /// counter-clockwise. A full circle is an arc with a sweep of 2π.
    pub fn push_arc(&mut self, style: StyleRole, arc: &Arc) {
        self.push_arc_along(style, arc, 0.0, true);
    }

    /// Appends one arc instance whose entity's path is `distance` long at
    /// the arc's counter-clockwise start and runs `forwards`
    /// (counter-clockwise) or back round it.
    pub fn push_arc_along(&mut self, style: StyleRole, arc: &Arc, distance: f64, forwards: bool) {
        let index = self.arc_count();
        let center = arc.center - self.origin;
        self.arcs.extend([
            center.x as f32,
            center.y as f32,
            arc.radius as f32,
            arc.start as f32,
            arc.sweep as f32,
        ]);
        self.arc_distances
            .extend([distance as f32, if forwards { 1.0 } else { -1.0 }]);
        extend_batches(&mut self.arc_batch_words, style, index);
    }

    /// Appends the stroke geometry of one entity. The single place that knows
    /// how each entity kind becomes segments, shared by scene and overlay. A
    /// block reference is its definition's entities, placed, read from
    /// `blocks` (ADR-0017).
    pub fn push_entity(&mut self, style: StyleRole, entity: &Entity, tables: Tables<'_>) {
        match entity {
            Entity::Line(line) => self.push_segment(style, line.start, line.end),
            Entity::Circle(circle) => {
                self.push_arc(
                    style,
                    &Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU),
                );
            }
            Entity::Arc(arc) => self.push_arc(style, &arc.arc()),
            // An ellipse is its chords (#181), the path distance running on
            // through them so a linetype does not start again at each.
            Entity::Ellipse(ellipse) => {
                let mut distance = 0.0;
                for chord in ellipse.ellipse().segments() {
                    self.push_segment_along(style, chord.start, chord.end, distance);
                    distance += chord.length();
                }
            }
            // Round caps on every segment meet at the vertices, so joints have
            // neither gaps nor spikes at any angle. Arc segments are drawn
            // as arcs, exactly like arc entities (ADR-0009). The path
            // distance runs on through the vertices — by chord along a
            // straight segment, by arc length along an arc segment — so a
            // linetype pattern does not start again at each.
            Entity::Polyline(polyline) => {
                let vertices = &polyline.vertices;
                let mut distance = 0.0;
                for (from, to) in polyline_spans(vertices.len(), polyline.closed, |index| {
                    polyline.bulge(index)
                }) {
                    let bulge = polyline.bulge(from);
                    match bulge_primitive(vertices[from], vertices[to], bulge) {
                        Primitive::Segment(segment) => {
                            self.push_segment_along(style, segment.start, segment.end, distance);
                            distance += segment.length();
                        }
                        Primitive::Arc(arc) => {
                            let length = arc.radius * arc.sweep;
                            // A clockwise segment runs from the arc's end
                            // back to its start: the path is `length` further
                            // along at the start than where this segment began.
                            let forwards = bulge > 0.0;
                            let at_start = if forwards {
                                distance
                            } else {
                                distance + length
                            };
                            self.push_arc_along(style, &arc, at_start, forwards);
                            distance += length;
                        }
                        Primitive::Circle(circle) => {
                            let full =
                                Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU);
                            self.push_arc_along(style, &full, distance, true);
                            distance += circle.radius * std::f64::consts::TAU;
                        }
                    }
                }
            }
            // A text is the strokes of its glyphs (ADR-0016): segments like
            // any other. The scene draws every text continuous, whatever
            // its linetype says, as drafting programs do.
            Entity::Text(text) => self.push_strokes(style, text.strokes()),
            Entity::MText(text) => self.push_strokes(style, text.strokes()),
            Entity::BlockRef(reference) => {
                for (part, _) in tables.blocks.flatten(reference, Properties::default()) {
                    self.push_entity(style, &part, tables);
                }
            }
            // A hatch is its fill — triangles for a solid one (ADR-0021),
            // the pattern's pieces as segments for a pattern — and its
            // boundary loops, drawn as polylines.
            Entity::Hatch(hatch) => {
                match hatch.style {
                    HatchStyle::Solid => {
                        for triangle in hatch.fill() {
                            self.push_triangle(style, triangle);
                        }
                    }
                    HatchStyle::Pattern { .. } => {
                        for line in hatch.lines().unwrap_or_default() {
                            self.push_segment(style, line.start, line.end);
                        }
                    }
                }
                for path in &hatch.loops {
                    let boundary = Entity::from(PolylineEntity::from(path.clone()));
                    self.push_entity(style, &boundary, tables);
                }
            }
            // A dimension is what its style lays out (ADR-0020): lines,
            // arrowhead outlines, an arc, and the strokes of its text.
            Entity::Dimension(dimension) => {
                let layout = dimension.layout(tables.dim_style(dimension.style));
                for segment in layout.segments() {
                    self.push_segment(style, segment.start, segment.end);
                }
                for arc in &layout.arcs {
                    self.push_arc(style, arc);
                }
                self.push_strokes(style, layout.text.strokes());
            }
        }
    }

    /// The segments of polylines, each at the start of its own path.
    fn push_strokes(&mut self, style: StyleRole, strokes: Vec<Vec<Point2>>) {
        for stroke in strokes {
            for pair in stroke.windows(2) {
                self.push_segment(style, pair[0], pair[1]);
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
            cad_document::Tables::none(),
        );
        frame.push_entity(
            StyleRole::Entity,
            &ArcEntity::new(p(100.0, 100.0), 2.0, FRAC_PI_2, PI).into(),
            cad_document::Tables::none(),
        );
        frame.push_entity(
            StyleRole::Preview,
            &CircleEntity::new(p(0.0, 0.0), 1.0).into(),
            cad_document::Tables::none(),
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
    /// #181: an ellipse is its chords, with the path distance running on
    /// through them.
    #[test]
    fn an_ellipse_becomes_chords_with_a_running_path_distance() {
        let mut frame = PackedFrame::default();
        let ellipse =
            cad_document::EllipseEntity::new(Point2::ORIGIN, cad_core::Vec2::new(100.0, 0.0), 0.5);
        frame.push_entity(StyleRole::Entity, &Entity::from(ellipse), Tables::none());
        assert_eq!(frame.segment_count(), cad_geometry::CHORDS_PER_TURN as u32);
        assert_eq!(frame.arc_count(), 0);
        let distances = frame.segment_distances();
        assert_eq!(distances[0], 0.0);
        assert!(distances.windows(2).all(|pair| pair[1] > pair[0]));
    }

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
        frame.push_entity(
            StyleRole::Entity,
            &slot.into(),
            cad_document::Tables::none(),
        );

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

    /// #138: a polyline's pattern does not restart at its vertices — the
    /// distance along the path runs on through them, by chord and by arc
    /// length, and a clockwise arc segment says so.
    #[test]
    fn path_distances_run_on_through_the_vertices_of_a_polyline() {
        use std::f64::consts::PI;

        use cad_document::{ArcEntity, CircleEntity, PolylineEntity};
        // Along the bottom, a counter-clockwise half circle up the right
        // (radius 2, length 2π), back along the top, then a *clockwise*
        // half circle down the left closing it.
        let slot = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
            vec![0.0, 1.0, 0.0, -1.0],
            true,
        );
        let mut frame = PackedFrame::default();
        frame.push_entity(
            StyleRole::Entity,
            &slot.into(),
            cad_document::Tables::none(),
        );

        let half = (2.0 * PI) as f32;
        assert_eq!(frame.segment_distances(), &[0.0, 10.0 + half]);
        // The right arc runs forwards from distance 10. The left one is
        // stored counter-clockwise from (0,0) to (0,4) but the path runs it
        // backwards, so at its counter-clockwise start the path is at the
        // closing end: 20 + 2 half circles.
        assert_eq!(frame.arc_distances(), &[10.0, 1.0, 20.0 + 2.0 * half, -1.0]);

        // Entities on their own start at zero, forwards.
        let mut frame = PackedFrame::default();
        frame.push_entity(
            StyleRole::Entity,
            &LineEntity::new(p(0.0, 0.0), p(5.0, 0.0)).into(),
            cad_document::Tables::none(),
        );
        frame.push_entity(
            StyleRole::Entity,
            &CircleEntity::new(p(0.0, 0.0), 3.0).into(),
            cad_document::Tables::none(),
        );
        frame.push_entity(
            StyleRole::Entity,
            &ArcEntity::new(p(0.0, 0.0), 3.0, 0.0, PI).into(),
            cad_document::Tables::none(),
        );
        frame.push_segment(StyleRole::Preview, p(0.0, 0.0), p(1.0, 1.0));
        assert_eq!(frame.segment_distances(), &[0.0, 0.0]);
        assert_eq!(frame.arc_distances(), &[0.0, 1.0, 0.0, 1.0]);
        assert_eq!(
            frame.arc_distances().len(),
            frame.arc_count() as usize * PackedFrame::FLOATS_PER_ARC_DISTANCE
        );
        frame.reset(Point2::ORIGIN);
        assert!(frame.segment_distances().is_empty() && frame.arc_distances().is_empty());
    }

    #[test]
    fn a_polyline_becomes_one_segment_per_edge_in_a_single_batch() {
        use cad_document::PolylineEntity;
        let corners = vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0)];
        let mut frame = PackedFrame::default();
        frame.push_entity(
            StyleRole::Entity,
            &PolylineEntity::new(corners.clone(), false).into(),
            cad_document::Tables::none(),
        );
        assert_eq!(frame.segments(), &[0.0, 0.0, 4.0, 0.0, 4.0, 0.0, 4.0, 3.0]);

        frame.reset(p(0.0, 0.0));
        frame.push_entity(
            StyleRole::Entity,
            &PolylineEntity::new(corners, true).into(),
            cad_document::Tables::none(),
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
        frame.push_entity(StyleRole::Entity, &line, cad_document::Tables::none());
        assert_eq!(frame.segments(), &[0.0, 0.0, 3.0, 4.0]);
    }
}
