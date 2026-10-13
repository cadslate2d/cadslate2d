//! The committed render scene (DESIGN §12.2–12.3).
//!
//! A scene has two parts with two lifetimes of their own (ADR-0014):
//!
//! - the **geometry**: one instance per segment and arc of every entity
//!   that is shown, and beside each instance the index of its style. It
//!   depends on the entities and on which layers are on, and is rebuilt
//!   when the drawing's *shown* revision changes;
//! - the **style table**: one row per distinct set of properties in use,
//!   saying what an instance with that index is drawn with. It depends on
//!   the layers too — `ByLayer` is resolved here — and is re-resolved when
//!   anything in the drawing changes. With it travels the **pattern
//!   table** (ADR-0015): the dash and gap lengths of every linetype a
//!   style names, already multiplied by `LTSCALE`.
//!
//! So recolouring a layer, or changing `LTSCALE`, changes a few bytes of
//! the tables and none of the geometry, however large the drawing is.

use std::collections::{BTreeSet, HashMap};

use cad_core::{Aabb, EntityId, Point2};
use cad_document::{
    Drawing, Entity, LayoutId, Linetype, LinetypeDef, LinetypeId, Paint, Properties, Revision,
    Weight,
};

use crate::{PackedFrame, StyleRole};

/// Entity IDs per chunk of the scene (ADR-0022): an edit re-packs the
/// chunks holding the entities it touched, and no other.
pub const CHUNK_IDS: u64 = 512;

/// A kind of committed instance, as the dirty ranges name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InstanceKind {
    Segments = 0,
    Arcs = 1,
    Triangles = 2,
}

/// A run of instances of one kind that changed since the last take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRange {
    pub kind: InstanceKind,
    pub first: u32,
    pub count: u32,
}

/// How many instances of each kind one chunk packed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ChunkCounts {
    segments: u32,
    arcs: u32,
    triangles: u32,
}

/// One chunk as packed: its counts, and the origin its instances are
/// offsets from (#188).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chunk {
    counts: ChunkCounts,
    origin: Point2,
}

/// How far, in drawing units, a chunk's geometry may lie from the origin it
/// is packed against: `f32` offsets this large resolve to under a hundredth
/// of a unit — a pixel at a hundred pixels per unit (#188). A chunk whose
/// extents reach past this from the origin in use takes its own, at its
/// extents' centre; one that stays within it shares the previous chunk's,
/// so a drawing of ordinary size is one run per kind, as before.
pub const MAX_ORIGIN_OFFSET: f64 = 100_000.0;

/// A run of consecutive chunks packed against one origin: what the adapter
/// draws under one translation (#188).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneRegion {
    pub origin: Point2,
    /// The first instance of each kind, in the order of [`InstanceKind`].
    pub first: [u32; 3],
    /// How many instances of each kind.
    pub count: [u32; 3],
}

/// Stable geometry derived from the canonical document.
///
/// A scene is a function of the drawing alone — there is no view parameter —
/// so pan, zoom, and pointer movement can never invalidate it (ADR-0003).
/// Its instances lie in chunk order: every entity's instances in the chunk
/// of its ID, chunks in ID order, with nothing between (ADR-0022).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderScene {
    revision: Revision,
    frame: PackedFrame,
    /// The style index of each segment instance, in instance order.
    segment_styles: Vec<u16>,
    /// The style index of each arc instance, in instance order.
    arc_styles: Vec<u16>,
    /// The style index of each triangle instance, in instance order.
    triangle_styles: Vec<u16>,
    /// The properties each style index stands for; appended to, never
    /// reordered, so an index stays what it was across repacks.
    properties: Vec<Properties>,
    style_indices: HashMap<Properties, u16>,
    style_revision: Revision,
    /// The style table as it crosses the boundary: see [`Self::styles`].
    styles: Vec<u8>,
    /// The pattern table: see [`Self::patterns`].
    patterns: Vec<f32>,
    /// What each chunk of [`CHUNK_IDS`] entity IDs packed, in chunk order,
    /// and the origin each is relative to.
    chunks: Vec<Chunk>,
    /// What changed since the last [`Self::take_dirty`], or everything.
    dirty: Vec<DirtyRange>,
    dirty_all: bool,
}

/// More dirty ranges than this are reported as everything: a frame that
/// uploads piece by piece past this point gains nothing.
const MAX_DIRTY_RANGES: usize = 64;

impl RenderScene {
    /// Bytes per row of the style table.
    pub const BYTES_PER_STYLE: usize = 8;
    /// The fourth byte of a row whose colour is the theme's foreground: the
    /// first three bytes mean nothing, and the adapter supplies the colour.
    pub const STYLE_FOREGROUND: u8 = 0;
    /// The fourth byte of a row whose first three bytes are red, green, blue.
    pub const STYLE_RGB: u8 = 255;
    /// The fifth byte of a row drawn at the theme's ordinary width. Any
    /// other value is a lineweight in hundredths of a millimetre, of which
    /// the heaviest standard one is 211.
    pub const STYLE_WEIGHT_DEFAULT: u8 = 255;
    /// Floats per row of the pattern table: the period, the number of
    /// lengths, then the lengths — dash, gap, dash, gap … — with zeros
    /// after the last.
    pub const FLOATS_PER_PATTERN: usize = 2 + LinetypeDef::MAX_LENGTHS;
    /// The pattern of a continuous line: row 0, whose period is zero. A
    /// drawing with more distinct patterns in use than a byte can index
    /// draws the surplus continuous.
    pub const PATTERN_CONTINUOUS: u8 = 0;

    fn empty(drawing: &Drawing) -> Self {
        Self {
            revision: drawing.shown_revision(),
            frame: PackedFrame::default(),
            segment_styles: Vec::new(),
            arc_styles: Vec::new(),
            triangle_styles: Vec::new(),
            properties: Vec::new(),
            style_indices: HashMap::new(),
            style_revision: drawing.revision(),
            styles: Vec::new(),
            patterns: Vec::new(),
            chunks: Vec::new(),
            dirty: Vec::new(),
            dirty_all: true,
        }
    }

    /// The runs of chunks packed against one origin, in order: each is
    /// drawn under its own translation (#188). A drawing within
    /// [`MAX_ORIGIN_OFFSET`] of one point is one region; a drawing that
    /// spans ten million units is one per end. Never empty: a scene with
    /// no chunks is one empty region at the frame's origin.
    pub fn regions(&self) -> Vec<SceneRegion> {
        let mut regions: Vec<SceneRegion> = Vec::new();
        let mut first = [0u32; 3];
        for chunk in &self.chunks {
            let count = [
                chunk.counts.segments,
                chunk.counts.arcs,
                chunk.counts.triangles,
            ];
            match regions.last_mut() {
                Some(region) if region.origin == chunk.origin => {
                    for (total, added) in region.count.iter_mut().zip(count) {
                        *total += added;
                    }
                }
                _ => regions.push(SceneRegion {
                    origin: chunk.origin,
                    first,
                    count,
                }),
            }
            for (next, added) in first.iter_mut().zip(count) {
                *next += added;
            }
        }
        if regions.is_empty() {
            regions.push(SceneRegion {
                origin: self.frame.origin(),
                first: [0; 3],
                count: [0; 3],
            });
        }
        regions
    }

    /// Whether every corner of `extents` lies within [`MAX_ORIGIN_OFFSET`]
    /// of `origin`.
    fn within_reach(origin: Point2, extents: &Aabb) -> bool {
        extents.corners().iter().all(|corner| {
            let offset = *corner - origin;
            offset.x.abs() <= MAX_ORIGIN_OFFSET && offset.y.abs() <= MAX_ORIGIN_OFFSET
        })
    }

    /// Packs `chunk` against the first of `candidates`, and — when its
    /// geometry reaches further from that than [`MAX_ORIGIN_OFFSET`], read
    /// off the offsets just packed — again against the first candidate
    /// within reach, or its own extents' centre. A drawing of ordinary
    /// size never packs twice. Returns the chunk's counts and origin.
    fn pack_chunk(&mut self, drawing: &Drawing, chunk: usize, candidates: &[Point2]) -> Chunk {
        let first = candidates.first().copied().unwrap_or(Point2::ORIGIN);
        let before = self.counts();
        self.frame.set_origin(first);
        self.pack_chunk_into_frame(drawing, chunk);
        let mut origin = first;
        let offsets = self
            .frame
            .offsets_since(before.segments, before.arcs, before.triangles);
        if let Some(offsets) = offsets {
            let extents = Aabb::from_points(
                first + (offsets.min() - Point2::ORIGIN),
                first + (offsets.max() - Point2::ORIGIN),
            );
            if !Self::within_reach(first, &extents) {
                origin = candidates[1..]
                    .iter()
                    .copied()
                    .find(|candidate| Self::within_reach(*candidate, &extents))
                    .unwrap_or_else(|| extents.center());
                self.frame
                    .truncate_to(before.segments, before.arcs, before.triangles);
                self.segment_styles.truncate(before.segments as usize);
                self.arc_styles.truncate(before.arcs as usize);
                self.triangle_styles.truncate(before.triangles as usize);
                self.frame.set_origin(origin);
                self.pack_chunk_into_frame(drawing, chunk);
            }
        }
        let after = self.counts();
        Chunk {
            counts: ChunkCounts {
                segments: after.segments - before.segments,
                arcs: after.arcs - before.arcs,
                triangles: after.triangles - before.triangles,
            },
            origin,
        }
    }

    /// Identifies the geometry: the drawing's shown revision it was built
    /// from. TypeScript re-uploads committed instance buffers only when
    /// this value changes.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn frame(&self) -> &PackedFrame {
        &self.frame
    }

    /// One style index per segment instance: a row of [`Self::styles`].
    pub fn segment_styles(&self) -> &[u16] {
        &self.segment_styles
    }

    /// One style index per arc instance.
    pub fn arc_styles(&self) -> &[u16] {
        &self.arc_styles
    }

    /// One style index per triangle instance (ADR-0021): a fill takes the
    /// row's colour and nothing else.
    pub fn triangle_styles(&self) -> &[u16] {
        &self.triangle_styles
    }

    /// Identifies the style table: the drawing revision it was resolved at.
    /// TypeScript re-uploads the table only when this value changes.
    pub fn style_revision(&self) -> Revision {
        self.style_revision
    }

    /// The style table: [`Self::BYTES_PER_STYLE`] bytes per row — `red,
    /// green, blue, kind, weight, pattern`, then two bytes kept for what a
    /// style gains next. `kind` is [`Self::STYLE_RGB`] or
    /// [`Self::STYLE_FOREGROUND`]; `weight` is a lineweight in hundredths of
    /// a millimetre or [`Self::STYLE_WEIGHT_DEFAULT`]; `pattern` is a row of
    /// [`Self::patterns`]. Row `i` is what an instance with style index `i`
    /// is drawn with.
    pub fn styles(&self) -> &[u8] {
        &self.styles
    }

    /// The pattern table: [`Self::FLOATS_PER_PATTERN`] floats per row, in
    /// drawing units with `LTSCALE` applied. Row [`Self::PATTERN_CONTINUOUS`]
    /// is always there and has no period; the others are the linetypes the
    /// styles name, each once.
    pub fn patterns(&self) -> &[f32] {
        &self.patterns
    }

    pub fn style_count(&self) -> usize {
        self.properties.len()
    }

    pub fn pattern_count(&self) -> usize {
        self.patterns.len() / Self::FLOATS_PER_PATTERN
    }

    /// The instances that changed since the last take, as ranges per kind
    /// — or, after a whole rebuild, one range per kind over everything —
    /// and clears them (ADR-0022). What a frame uploads: the ranges, with
    /// `bufferSubData`, or the whole buffers.
    pub fn take_dirty(&mut self) -> Vec<DirtyRange> {
        let ranges = if self.dirty_all || self.dirty.len() > MAX_DIRTY_RANGES {
            let whole = |kind, count| DirtyRange {
                kind,
                first: 0,
                count,
            };
            vec![
                whole(InstanceKind::Segments, self.frame.segment_count()),
                whole(InstanceKind::Arcs, self.frame.arc_count()),
                whole(InstanceKind::Triangles, self.frame.triangle_count()),
            ]
        } else {
            self.dirty.clone()
        };
        self.dirty.clear();
        self.dirty_all = false;
        ranges
    }

    /// The whole scene, afresh: every chunk packed in order, each against
    /// the origin of the chunk before it while that is within reach, and
    /// against its own extents' centre otherwise (#188).
    fn rebuild_all(&mut self, drawing: &Drawing) {
        // Centre the first origin on the drawing so f32 offsets keep the
        // most precision where the geometry actually is.
        let origin = drawing
            .extents()
            .map_or(Point2::ORIGIN, |extents| extents.center());
        self.frame.reset(origin);
        self.segment_styles.clear();
        self.arc_styles.clear();
        self.triangle_styles.clear();
        self.properties.clear();
        self.style_indices.clear();
        self.chunks.clear();
        let chunk_count = drawing.id_bound().div_ceil(CHUNK_IDS) as usize;
        let mut previous = origin;
        for chunk in 0..chunk_count {
            let packed = self.pack_chunk(drawing, chunk, &[previous]);
            previous = packed.origin;
            self.chunks.push(packed);
        }
        self.frame.batch_everything(StyleRole::Entity);
        self.dirty.clear();
        self.dirty_all = true;
    }

    /// Repacks only `chunks`, each in its place, moving what follows.
    fn repack(&mut self, drawing: &Drawing, chunks: &BTreeSet<usize>) {
        let needed = drawing.id_bound().div_ceil(CHUNK_IDS) as usize;
        if self.chunks.len() < needed {
            // A new chunk starts from the origin of the one before it.
            let origin = self
                .chunks
                .last()
                .map_or(self.frame.origin(), |chunk| chunk.origin);
            self.chunks.resize(
                needed,
                Chunk {
                    counts: ChunkCounts::default(),
                    origin,
                },
            );
        }
        for &chunk in chunks {
            if chunk >= self.chunks.len() {
                continue;
            }
            // Pack the chunk on its own, then splice it over the old one.
            // It keeps its origin while its geometry stays within reach of
            // it — so what was there is where it was — and takes the
            // previous chunk's, or its own centre, when it does not.
            let kept = self.chunks[chunk].origin;
            let before = chunk
                .checked_sub(1)
                .map_or(kept, |previous| self.chunks[previous].origin);
            let mut scratch = RenderScene::scratch(self, kept);
            let packed = scratch.pack_chunk(drawing, chunk, &[kept, before]);
            let origin = packed.origin;
            self.properties = std::mem::take(&mut scratch.properties);
            self.style_indices = std::mem::take(&mut scratch.style_indices);
            let new = packed.counts;
            let old = self.chunks[chunk].counts;
            let offset = self.chunks[..chunk]
                .iter()
                .fold(ChunkCounts::default(), |sum, chunk| ChunkCounts {
                    segments: sum.segments + chunk.counts.segments,
                    arcs: sum.arcs + chunk.counts.arcs,
                    triangles: sum.triangles + chunk.counts.triangles,
                });
            let splice = |into: &mut Vec<f32>, from: &[f32], per: usize, at: u32, old: u32| {
                let start = at as usize * per;
                into.splice(start..start + old as usize * per, from.iter().copied());
            };
            let splice_styles = |into: &mut Vec<u16>, from: &[u16], at: u32, old: u32| {
                into.splice(at as usize..(at + old) as usize, from.iter().copied());
            };
            let seg = PackedFrame::FLOATS_PER_SEGMENT;
            let arc = PackedFrame::FLOATS_PER_ARC;
            let tri = PackedFrame::FLOATS_PER_TRIANGLE;
            let dist = PackedFrame::FLOATS_PER_ARC_DISTANCE;
            splice(
                self.frame.segments_mut(),
                scratch.frame.segments(),
                seg,
                offset.segments,
                old.segments,
            );
            splice(
                self.frame.segment_distances_mut(),
                scratch.frame.segment_distances(),
                1,
                offset.segments,
                old.segments,
            );
            splice_styles(
                &mut self.segment_styles,
                &scratch.segment_styles,
                offset.segments,
                old.segments,
            );
            splice(
                self.frame.arcs_mut(),
                scratch.frame.arcs(),
                arc,
                offset.arcs,
                old.arcs,
            );
            splice(
                self.frame.arc_distances_mut(),
                scratch.frame.arc_distances(),
                dist,
                offset.arcs,
                old.arcs,
            );
            splice_styles(
                &mut self.arc_styles,
                &scratch.arc_styles,
                offset.arcs,
                old.arcs,
            );
            splice(
                self.frame.triangles_mut(),
                scratch.frame.triangles(),
                tri,
                offset.triangles,
                old.triangles,
            );
            splice_styles(
                &mut self.triangle_styles,
                &scratch.triangle_styles,
                offset.triangles,
                old.triangles,
            );
            self.chunks[chunk] = Chunk {
                counts: new,
                origin,
            };
            // What to upload: the chunk's run when its size held; from the
            // chunk to the end when what follows moved.
            let totals = self.counts();
            let mut note = |kind, first: u32, old: u32, new: u32, total: u32| {
                if old == new && new == 0 {
                    return;
                }
                let count = if old == new { new } else { total - first };
                self.dirty.push(DirtyRange { kind, first, count });
            };
            note(
                InstanceKind::Segments,
                offset.segments,
                old.segments,
                new.segments,
                totals.segments,
            );
            note(
                InstanceKind::Arcs,
                offset.arcs,
                old.arcs,
                new.arcs,
                totals.arcs,
            );
            note(
                InstanceKind::Triangles,
                offset.triangles,
                old.triangles,
                new.triangles,
                totals.triangles,
            );
        }
        self.frame.batch_everything(StyleRole::Entity);
    }

    /// An empty scene sharing this one's style table, to pack one chunk in.
    fn scratch(&mut self, origin: Point2) -> Self {
        let mut frame = PackedFrame::default();
        frame.reset(origin);
        Self {
            revision: self.revision,
            frame,
            segment_styles: Vec::new(),
            arc_styles: Vec::new(),
            triangle_styles: Vec::new(),
            properties: std::mem::take(&mut self.properties),
            style_indices: std::mem::take(&mut self.style_indices),
            style_revision: self.style_revision,
            styles: Vec::new(),
            patterns: Vec::new(),
            chunks: Vec::new(),
            dirty: Vec::new(),
            dirty_all: false,
        }
    }

    fn counts(&self) -> ChunkCounts {
        ChunkCounts {
            segments: self.frame.segment_count(),
            arcs: self.frame.arc_count(),
            triangles: self.frame.triangle_count(),
        }
    }

    /// Appends the shown entities of `chunk`, in ID order.
    fn pack_chunk_into_frame(&mut self, drawing: &Drawing, chunk: usize) {
        let first = chunk as u64 * CHUNK_IDS;
        for raw in first..first + CHUNK_IDS {
            let id = EntityId::from_raw(raw);
            let Some(entity) = drawing.entity(id) else {
                continue;
            };
            // Objects on a layer that is off are not drawn: no instances.
            // Nor is what is on a layout: the scene is the model's (#191).
            if !drawing.is_shown_in(id, LayoutId::MODEL) {
                continue;
            }
            let properties = drawing.properties(id).unwrap_or_default();
            // A block reference is its definition's entities, each drawn
            // with its own properties resolved against the reference's
            // (ADR-0017): one style per distinct result.
            let parts: Vec<(Entity, Properties)> = match entity {
                Entity::BlockRef(reference) => drawing.blocks().flatten(reference, properties),
                other => vec![(other.clone(), properties)],
            };
            for (entity, properties) in parts {
                self.push_part(drawing, &entity, properties);
            }
        }
    }

    /// One drawn entity with the properties it is drawn with.
    fn push_part(&mut self, drawing: &Drawing, entity: &Entity, mut properties: Properties) {
        // A linetype is for curves: a text is drawn continuous, or its
        // glyphs would fall apart into dashes. Nor a dimension's text, nor
        // a hatch, whose pattern carries its own dashes.
        if matches!(
            entity,
            Entity::Text(_)
                | Entity::MText(_)
                | Entity::Dimension(_)
                | Entity::Hatch(_)
                | Entity::Viewport(_)
        ) {
            properties.linetype = Linetype::CONTINUOUS;
        }
        let style = *self.style_indices.entry(properties).or_insert_with(|| {
            // More distinct styles than indices: the surplus share the
            // last one rather than wrapping round onto the first.
            let index = self.properties.len().min(usize::from(u16::MAX));
            if index == self.properties.len() {
                self.properties.push(properties);
            }
            index as u16
        });
        let before = self.counts();
        self.frame
            .push_entity(StyleRole::Entity, entity, drawing.tables());
        let after = self.counts();
        let added = |before: u32, after: u32| (after - before) as usize;
        self.segment_styles.extend(std::iter::repeat_n(
            style,
            added(before.segments, after.segments),
        ));
        self.arc_styles
            .extend(std::iter::repeat_n(style, added(before.arcs, after.arcs)));
        self.triangle_styles.extend(std::iter::repeat_n(
            style,
            added(before.triangles, after.triangles),
        ));
    }

    /// Resolves every style in use against the layers, linetypes, and
    /// settings as they are now.
    fn resolve_styles(&mut self, drawing: &Drawing) {
        self.styles.clear();
        self.patterns.clear();
        self.patterns
            .extend(std::iter::repeat_n(0.0, Self::FLOATS_PER_PATTERN));
        let mut pattern_rows: HashMap<LinetypeId, u8> = HashMap::new();
        for properties in &self.properties {
            let layer = drawing.layers().get(properties.layer);
            let paint = layer.map_or(Paint::Foreground, |layer| properties.color.paint_on(layer));
            let weight = layer.map_or(Weight::Default, |layer| {
                properties.lineweight.weight_on(layer)
            });
            let linetype =
                layer.map_or(LinetypeId::FIRST, |layer| properties.linetype.row_on(layer));
            let pattern = match drawing.linetypes().get(linetype) {
                Some(def) if !def.is_continuous() => {
                    *pattern_rows.entry(linetype).or_insert_with(|| {
                        let row = self.patterns.len() / Self::FLOATS_PER_PATTERN;
                        if row > usize::from(u8::MAX) {
                            return Self::PATTERN_CONTINUOUS;
                        }
                        let lengths = def.pattern.iter().map(|length| *length * drawing.ltscale());
                        self.patterns
                            .push((def.period() * drawing.ltscale()) as f32);
                        self.patterns.push(def.pattern.len() as f32);
                        self.patterns.extend(lengths.map(|length| length as f32));
                        self.patterns.extend(std::iter::repeat_n(
                            0.0,
                            LinetypeDef::MAX_LENGTHS - def.pattern.len(),
                        ));
                        row as u8
                    })
                }
                _ => Self::PATTERN_CONTINUOUS,
            };
            let [red, green, blue, kind] = match paint {
                Paint::Rgb([red, green, blue]) => [red, green, blue, Self::STYLE_RGB],
                Paint::Foreground => [0, 0, 0, Self::STYLE_FOREGROUND],
            };
            let weight = match weight {
                Weight::Default => Self::STYLE_WEIGHT_DEFAULT,
                // The standard widths all fit; anything heavier is drawn as
                // the heaviest that does.
                Weight::Hundredths(hundredths) => {
                    hundredths.min(u16::from(Self::STYLE_WEIGHT_DEFAULT) - 1) as u8
                }
            };
            self.styles
                .extend([red, green, blue, kind, weight, pattern, 0, 0]);
        }
        self.style_revision = drawing.revision();
    }
}

/// Holds the current scene and rebuilds each part of it only when what that
/// part depends on has changed since the last request — and of the
/// geometry, only the chunks holding what changed (ADR-0022).
#[derive(Debug, Default)]
pub struct SceneCache {
    scene: Option<RenderScene>,
    builds: u64,
    /// Of the builds, those that repacked chunks rather than everything.
    partial_builds: u64,
    style_builds: u64,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The scene for `drawing`. The geometry is rebuilt if and only if what
    /// is shown changed — the chunks that hold the changed entities, when
    /// the drawing can say which, else all of it; the style table is
    /// re-resolved if and only if the drawing changed at all.
    pub fn scene(&mut self, drawing: &Drawing) -> &RenderScene {
        let stale = self
            .scene
            .as_ref()
            .is_none_or(|scene| scene.revision != drawing.shown_revision());
        if stale {
            let changed = self
                .scene
                .as_ref()
                .and_then(|scene| drawing.shown_changes_since(scene.revision));
            match (self.scene.as_mut(), changed) {
                (Some(scene), Some(ids)) => {
                    let chunks: BTreeSet<usize> = ids
                        .iter()
                        .map(|id| (id.raw() / CHUNK_IDS) as usize)
                        .collect();
                    scene.repack(drawing, &chunks);
                    scene.revision = drawing.shown_revision();
                    self.partial_builds += 1;
                }
                _ => {
                    // The previous scene's buffers are reused, to avoid reallocating.
                    let mut scene = self
                        .scene
                        .take()
                        .unwrap_or_else(|| RenderScene::empty(drawing));
                    scene.rebuild_all(drawing);
                    scene.revision = drawing.shown_revision();
                    self.scene = Some(scene);
                }
            }
            self.builds += 1;
            // New geometry may have new styles: resolve them whatever the
            // drawing's revision says.
            let scene = self.scene.as_mut().expect("the scene was just built");
            scene.resolve_styles(drawing);
            self.style_builds += 1;
        }
        let scene = self.scene.as_mut().expect("the scene was just built");
        if scene.style_revision != drawing.revision() {
            scene.resolve_styles(drawing);
            self.style_builds += 1;
        }
        scene
    }

    /// The changed instance ranges since the last take (see
    /// [`RenderScene::take_dirty`]); everything, before the first scene.
    pub fn take_dirty(&mut self) -> Vec<DirtyRange> {
        self.scene
            .as_mut()
            .map_or_else(Vec::new, RenderScene::take_dirty)
    }

    /// How many times the geometry has been built, whole or in part. Tests
    /// use this to prove that pointer activity — and a change to how a
    /// layer looks — does not regenerate committed geometry.
    pub fn build_count(&self) -> u64 {
        self.builds
    }

    /// Of the builds, how many repacked only the chunks that changed.
    pub fn partial_build_count(&self) -> u64 {
        self.partial_builds
    }

    /// How many times the style table has been resolved.
    pub fn style_build_count(&self) -> u64 {
        self.style_builds
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
        tx.insert(entity).unwrap();
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
        assert_eq!(scene.revision(), drawing.shown_revision());
        assert_eq!(scene.style_count(), 0);
        assert!(scene.styles().is_empty());
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
        assert_eq!(cache.scene(&drawing).revision(), drawing.shown_revision());
        assert_eq!(cache.scene(&drawing).style_revision(), drawing.revision());
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

    // ── styles (#134, ADR-0014) ──

    use cad_document::{CircleEntity, Color, Entity, Layer, LayerId, PolylineEntity};

    const RED: [u8; 4] = [255, 0, 0, RenderScene::STYLE_RGB];
    const BLUE: [u8; 4] = [0, 0, 255, RenderScene::STYLE_RGB];
    const FOREGROUND: [u8; 4] = [0, 0, 0, RenderScene::STYLE_FOREGROUND];

    /// The colour of every row: red, green, blue, kind.
    fn rows(scene: &RenderScene) -> Vec<[u8; 4]> {
        let (rows, rest) = scene
            .styles()
            .as_chunks::<{ RenderScene::BYTES_PER_STYLE }>();
        assert!(rest.is_empty(), "whole rows only");
        rows.iter()
            .map(|row| [row[0], row[1], row[2], row[3]])
            .collect()
    }

    /// The weight byte of every row.
    fn weights(scene: &RenderScene) -> Vec<u8> {
        let (rows, _) = scene
            .styles()
            .as_chunks::<{ RenderScene::BYTES_PER_STYLE }>();
        rows.iter().map(|row| row[4]).collect()
    }

    fn on(layer: u32) -> Properties {
        Properties::on(LayerId::from_raw(layer))
    }

    /// Layers `red` (1) and `blue` (2) besides `0`, and some of everything.
    fn coloured() -> Drawing {
        let layers = [
            Layer {
                color: Color::Index(1),
                ..Layer::named("red")
            },
            Layer {
                color: Color::Index(5),
                ..Layer::named("blue")
            },
        ];
        let square = PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 10.0),
            ],
            vec![0.0, 1.0, 0.0],
            true,
        );
        Drawing::from_parts(
            layers,
            [],
            [
                (Entity::Line(line(0.0, 0.0, 100.0, 0.0)), on(1)),
                (
                    Entity::Line(line(0.0, 5.0, 100.0, 5.0)),
                    Properties::default(),
                ),
                (
                    CircleEntity::new(Point2::new(50.0, 50.0), 10.0).into(),
                    on(2),
                ),
                // Two straight segments and an arc, all red.
                (square.into(), on(1)),
                // On the red layer, but with a colour of its own.
                (
                    Entity::Line(line(0.0, 9.0, 100.0, 9.0)),
                    Properties {
                        color: Color::Index(5),
                        ..on(1)
                    },
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn every_instance_has_the_style_of_its_entity() {
        let drawing = coloured();
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        // Styles are numbered in the order they are first met: red layer,
        // layer 0, blue layer, blue on the red layer.
        assert_eq!(rows(scene), vec![RED, FOREGROUND, BLUE, BLUE]);
        assert_eq!(scene.style_count(), 4);
        // Segments: the red line, the plain line, the polyline's two, the blue line.
        assert_eq!(scene.segment_styles(), &[0, 1, 0, 0, 3]);
        // Arcs: the circle, then the polyline's arc.
        assert_eq!(scene.arc_styles(), &[2, 0]);
        // One index per instance, and every index a row of the table.
        assert_eq!(
            scene.segment_styles().len(),
            scene.frame().segment_count() as usize
        );
        assert_eq!(scene.arc_styles().len(), scene.frame().arc_count() as usize);
        let all = scene.segment_styles().iter().chain(scene.arc_styles());
        assert!(
            all.clone()
                .all(|style| usize::from(*style) < scene.style_count())
        );
        assert_eq!(
            scene.styles().len(),
            scene.style_count() * RenderScene::BYTES_PER_STYLE
        );
    }

    #[test]
    fn the_same_coloured_drawing_always_yields_the_same_scene() {
        let (a, b) = (coloured(), coloured());
        let (mut cache_a, mut cache_b) = (SceneCache::new(), SceneCache::new());
        let (scene_a, scene_b) = (cache_a.scene(&a), cache_b.scene(&b));
        assert_eq!(scene_a.frame(), scene_b.frame());
        assert_eq!(scene_a.segment_styles(), scene_b.segment_styles());
        assert_eq!(scene_a.arc_styles(), scene_b.arc_styles());
        assert_eq!(scene_a.styles(), scene_b.styles());
    }

    fn recolour(drawing: &mut Drawing, name: &str, color: Color) {
        let id = drawing.layers().find(name).unwrap();
        let mut layer = drawing.layers().get(id).cloned().unwrap();
        layer.color = color;
        let mut tx = drawing.transaction();
        tx.replace_layer(id, layer).unwrap();
        tx.commit();
    }

    /// The acceptance criterion: a layer colour change touches the table
    /// and not the geometry.
    #[test]
    fn recolouring_a_layer_changes_the_table_and_not_the_geometry() {
        let mut drawing = coloured();
        let mut cache = SceneCache::new();
        let before = cache.scene(&drawing).clone();

        recolour(&mut drawing, "red", Color::Index(3));
        let after = cache.scene(&drawing).clone();
        assert_eq!(
            cache_counts(&cache),
            (1, 2),
            "one geometry build, two tables"
        );
        assert_eq!(
            after.revision(),
            before.revision(),
            "the geometry is the same geometry"
        );
        assert_eq!(after.frame(), before.frame());
        assert_eq!(after.segment_styles(), before.segment_styles());
        assert_ne!(after.style_revision(), before.style_revision());
        let green = [0, 255, 0, RenderScene::STYLE_RGB];
        assert_eq!(
            rows(&after),
            vec![green, FOREGROUND, BLUE, BLUE],
            "only ByLayer rows follow"
        );

        // Colour 7 is the foreground, whichever layer has it.
        recolour(&mut drawing, "red", Color::Index(7));
        assert_eq!(rows(cache.scene(&drawing))[0], FOREGROUND);
        // Undo walks the table back.
        drawing.undo();
        assert_eq!(rows(cache.scene(&drawing))[0], green);
        assert_eq!(cache.build_count(), 1, "still the first geometry");
    }

    fn cache_counts(cache: &SceneCache) -> (u64, u64) {
        (cache.build_count(), cache.style_build_count())
    }

    #[test]
    fn renaming_or_locking_a_layer_rebuilds_no_geometry() {
        let mut drawing = coloured();
        let mut cache = SceneCache::new();
        cache.scene(&drawing);
        let id = drawing.layers().find("blue").unwrap();
        let mut layer = drawing.layers().get(id).cloned().unwrap();
        layer.name = "navy".to_owned();
        layer.locked = true;
        let mut tx = drawing.transaction();
        tx.replace_layer(id, layer).unwrap();
        tx.commit();
        cache.scene(&drawing);
        assert_eq!(cache.build_count(), 1);
    }

    #[test]
    fn a_layer_switched_off_takes_its_instances_and_its_style_away() {
        let mut drawing = coloured();
        let mut cache = SceneCache::new();
        assert_eq!(cache.scene(&drawing).frame().arc_count(), 2);

        let id = drawing.layers().find("blue").unwrap();
        let mut layer = drawing.layers().get(id).cloned().unwrap();
        layer.on = false;
        let mut tx = drawing.transaction();
        tx.replace_layer(id, layer).unwrap();
        tx.commit();

        let scene = cache.scene(&drawing);
        assert_eq!(scene.frame().arc_count(), 1, "the circle is gone");
        assert_eq!(
            rows(scene),
            vec![RED, FOREGROUND, BLUE],
            "and the style only it had"
        );
        assert_eq!(cache.build_count(), 2);
    }

    #[test]
    fn a_change_of_properties_restyles_the_instance() {
        let mut drawing = coloured();
        let mut cache = SceneCache::new();
        cache.scene(&drawing);
        let plain = drawing
            .records()
            .find(|(_, record)| record.properties == Properties::default())
            .map(|(id, _)| id)
            .unwrap();
        let mut tx = drawing.transaction();
        tx.set_properties(plain, on(2)).unwrap();
        tx.commit();
        let scene = cache.scene(&drawing);
        // The row nobody uses any more stays, so every other instance's
        // index still means what it did (ADR-0022); the new style is a
        // new row.
        assert_eq!(rows(scene), vec![RED, FOREGROUND, BLUE, BLUE]);
        assert_eq!(scene.segment_styles(), &[0, 2, 0, 0, 3]);
        assert_eq!(cache.partial_build_count(), 1);
    }

    /// #151 (ADR-0003, extended): the scene's one origin is the centre of
    /// the extents, and `f32` offsets from it resolve to a unit at 10⁷. A
    /// drawing spanning that far is placed to within a unit at its far end
    /// — the bound the ADR records — and exactly where it lies near the
    /// origin of the scene.
    #[test]
    fn a_drawing_spanning_ten_million_units_is_placed_to_a_unit_at_its_far_end() {
        let far = 1e7;
        let drawing = Drawing::from_entities([
            Entity::from(LineEntity::new(
                Point2::new(0.0, 0.0),
                Point2::new(100.0, 0.0),
            )),
            Entity::from(LineEntity::new(
                Point2::new(far, far),
                Point2::new(far + 100.0, far),
            )),
        ]);
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        let origin = scene.frame().origin();
        let placed: Vec<Point2> = scene
            .frame()
            .segments()
            .chunks(PackedFrame::FLOATS_PER_SEGMENT)
            .flat_map(|s| {
                [
                    origin + cad_core::Vec2::new(f64::from(s[0]), f64::from(s[1])),
                    origin + cad_core::Vec2::new(f64::from(s[2]), f64::from(s[3])),
                ]
            })
            .collect();
        let truth = [
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(far, far),
            Point2::new(far + 100.0, far),
        ];
        for (point, exact) in placed.iter().zip(truth) {
            let off = point.distance_to(exact);
            assert!(off <= 1.0, "{exact:?} placed {off} off");
        }
        // The origin is the centre of the extents: both ends are half the
        // span from it, at the same resolution.
        assert!((origin.x - (far + 100.0) / 2.0).abs() < 1e-6);
        assert!((origin.y - far / 2.0).abs() < 1e-6);
    }

    /// #188: a chunk far from the chunk before it is packed against its
    /// own origin, and the regions say which run is which; a drawing of
    /// ordinary size is one region, as before.
    #[test]
    fn chunks_out_of_reach_of_the_previous_origin_take_their_own_and_make_a_region() {
        let far = 1e7;
        let near = LineEntity::new(Point2::new(0.0, 0.0), Point2::new(100.0, 0.0));
        let mut entities: Vec<Entity> = vec![near.into()];
        // Fill the first chunk — IDs start at 1 — so the far lines land
        // in the second.
        for index in 2..CHUNK_IDS {
            let y = index as f64;
            entities.push(LineEntity::new(Point2::new(0.0, y), Point2::new(10.0, y)).into());
        }
        entities.push(LineEntity::new(Point2::new(far, far), Point2::new(far + 100.0, far)).into());
        entities.push(
            LineEntity::new(Point2::new(far, far + 60.0), Point2::new(far, far - 60.0)).into(),
        );
        let drawing = Drawing::from_entities(entities);
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        let regions = scene.regions();
        assert_eq!(regions.len(), 2, "{regions:?}");
        assert_eq!(regions[0].first, [0, 0, 0]);
        let near_count = CHUNK_IDS as u32 - 1;
        assert_eq!(regions[0].count, [near_count, 0, 0]);
        assert_eq!(regions[1].first, [near_count, 0, 0]);
        assert_eq!(regions[1].count, [2, 0, 0]);
        // The far chunk's origin is its own extents' centre, read off the
        // offsets packed from the near origin — a unit or so out at 10⁷,
        // which does not matter: what matters is how far the geometry is
        // from it, and that is tens of units …
        assert!((regions[1].origin.x - (far + 50.0)).abs() < 2.0);
        assert!((regions[1].origin.y - far).abs() < 2.0);
        // … so its geometry is placed to a hundredth of a unit: a pixel
        // at a hundred pixels per unit.
        let segments = scene.frame().segments();
        let placed = |index: usize, region: &SceneRegion| -> [Point2; 2] {
            let s = &segments[index * PackedFrame::FLOATS_PER_SEGMENT..];
            [
                region.origin + cad_core::Vec2::new(f64::from(s[0]), f64::from(s[1])),
                region.origin + cad_core::Vec2::new(f64::from(s[2]), f64::from(s[3])),
            ]
        };
        let [a, b] = placed(near_count as usize, &regions[1]);
        assert!(a.distance_to(Point2::new(far, far)) < 0.01, "{a:?}");
        assert!(b.distance_to(Point2::new(far + 100.0, far)) < 0.01, "{b:?}");
        let [c, d] = placed(near_count as usize + 1, &regions[1]);
        assert!(c.distance_to(Point2::new(far, far + 60.0)) < 0.01);
        assert!(d.distance_to(Point2::new(far, far - 60.0)) < 0.01);
        let [e, f] = placed(0, &regions[0]);
        assert!(e.distance_to(Point2::new(0.0, 0.0)) < 0.01);
        assert!(f.distance_to(Point2::new(100.0, 0.0)) < 0.01);

        // A drawing of ordinary size, however many chunks: one region.
        let mut many = Vec::new();
        for index in 0..CHUNK_IDS * 3 {
            let y = index as f64;
            many.push(Entity::from(LineEntity::new(
                Point2::new(0.0, y),
                Point2::new(10.0, y),
            )));
        }
        let drawing = Drawing::from_entities(many);
        let regions = SceneCache::new().scene(&drawing).regions();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].count, [CHUNK_IDS as u32 * 3, 0, 0]);
        // A scene with nothing in it is one empty region.
        let regions = SceneCache::new().scene(&Drawing::new()).regions();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].count, [0, 0, 0]);
    }

    /// #188: an edit keeps a chunk's origin while its geometry stays within
    /// reach of it, and moves it when the chunk is drawn far away.
    #[test]
    fn a_repacked_chunk_keeps_its_origin_within_reach_and_moves_it_beyond() {
        let mut drawing = Drawing::from_entities([Entity::from(LineEntity::new(
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
        ))]);
        let mut cache = SceneCache::new();
        let origin = cache.scene(&drawing).regions()[0].origin;
        assert_eq!(origin, Point2::new(50.0, 0.0));

        let mut tx = drawing.transaction();
        tx.insert(LineEntity::new(
            Point2::new(0.0, 1000.0),
            Point2::new(100.0, 1000.0),
        ))
        .unwrap();
        tx.commit();
        let regions = cache.scene(&drawing).regions();
        assert_eq!(cache.partial_build_count(), 1);
        assert_eq!(regions[0].origin, origin, "within reach: the same origin");

        let mut tx = drawing.transaction();
        tx.insert(LineEntity::new(
            Point2::new(0.0, 1e7),
            Point2::new(100.0, 1e7),
        ))
        .unwrap();
        tx.commit();
        let regions = cache.scene(&drawing).regions();
        assert_eq!(cache.partial_build_count(), 2);
        assert_ne!(
            regions[0].origin, origin,
            "out of reach: the chunk's own centre"
        );
        assert!((regions[0].origin.y - 5e6).abs() < 1.0);
    }

    #[test]
    fn pointer_free_requests_resolve_nothing() {
        let drawing = coloured();
        let mut cache = SceneCache::new();
        for _ in 0..100 {
            cache.scene(&drawing);
        }
        assert_eq!(cache_counts(&cache), (1, 1));
    }

    // ── lineweights (#136) ──

    #[test]
    fn a_style_row_carries_the_resolved_lineweight() {
        use cad_document::Lineweight;

        let layers = [Layer {
            lineweight: Lineweight::Hundredths(50),
            ..Layer::named("heavy")
        }];
        let own = Properties {
            lineweight: Lineweight::Hundredths(211),
            ..on(1)
        };
        let default_on_heavy = Properties {
            lineweight: Lineweight::Default,
            ..on(1)
        };
        let mut drawing = Drawing::from_parts(
            layers,
            [],
            [
                (
                    Entity::Line(line(0.0, 0.0, 10.0, 0.0)),
                    Properties::default(),
                ),
                (Entity::Line(line(0.0, 1.0, 10.0, 1.0)), on(1)),
                (Entity::Line(line(0.0, 2.0, 10.0, 2.0)), own),
                (Entity::Line(line(0.0, 3.0, 10.0, 3.0)), default_on_heavy),
            ],
        )
        .unwrap();
        let mut cache = SceneCache::new();
        let default = RenderScene::STYLE_WEIGHT_DEFAULT;
        // Layer 0 has the default weight; ByLayer on `heavy` is 0.50 mm; an
        // object's own weight wins; and Default is not ByLayer.
        assert_eq!(
            weights(cache.scene(&drawing)),
            vec![default, 50, 211, default]
        );
        assert_eq!(cache.scene(&drawing).segment_styles(), &[0, 1, 2, 3]);

        // Changing the layer's lineweight is a table change, like its colour.
        let id = drawing.layers().find("heavy").unwrap();
        let mut layer = drawing.layers().get(id).cloned().unwrap();
        layer.lineweight = Lineweight::Hundredths(100);
        let mut tx = drawing.transaction();
        tx.replace_layer(id, layer).unwrap();
        tx.commit();
        assert_eq!(
            weights(cache.scene(&drawing)),
            vec![default, 100, 211, default]
        );
        assert_eq!(cache.build_count(), 1, "no geometry was rebuilt");
    }

    // ── linetypes (#138) ──

    /// The pattern byte of every row.
    fn patterns_of(scene: &RenderScene) -> Vec<u8> {
        let (rows, _) = scene
            .styles()
            .as_chunks::<{ RenderScene::BYTES_PER_STYLE }>();
        rows.iter().map(|row| row[5]).collect()
    }

    fn pattern_rows(scene: &RenderScene) -> Vec<Vec<f32>> {
        let (rows, rest) = scene
            .patterns()
            .as_chunks::<{ RenderScene::FLOATS_PER_PATTERN }>();
        assert!(rest.is_empty(), "whole rows only");
        rows.iter().map(|row| row.to_vec()).collect()
    }

    #[test]
    fn a_style_row_names_its_pattern_and_the_table_holds_each_once() {
        use cad_document::{Linetype, LinetypeDef, LinetypeId};

        let (dashed, hidden) = (LinetypeId::from_raw(1), LinetypeId::from_raw(2));
        let layers = [Layer {
            linetype: Linetype::Named(dashed),
            ..Layer::named("dashed")
        }];
        let linetypes = [
            LinetypeDef::built_in_named("Dashed").unwrap(),
            LinetypeDef::built_in_named("Hidden").unwrap(),
        ];
        let with = |linetype: Linetype, layer: u32| Properties {
            linetype,
            ..on(layer)
        };
        let mut drawing = Drawing::from_parts(
            layers,
            linetypes,
            [
                // Continuous by layer 0; dashed by its layer; its own
                // hidden on that layer; dashed by name on layer 0; and
                // continuous by name on the dashed layer.
                (
                    Entity::Line(line(0.0, 0.0, 10.0, 0.0)),
                    Properties::default(),
                ),
                (Entity::Line(line(0.0, 1.0, 10.0, 1.0)), on(1)),
                (
                    Entity::Line(line(0.0, 2.0, 10.0, 2.0)),
                    with(Linetype::Named(hidden), 1),
                ),
                (
                    Entity::Line(line(0.0, 3.0, 10.0, 3.0)),
                    with(Linetype::Named(dashed), 0),
                ),
                (
                    Entity::Line(line(0.0, 4.0, 10.0, 4.0)),
                    with(Linetype::CONTINUOUS, 1),
                ),
                (
                    Entity::Line(line(0.0, 5.0, 10.0, 5.0)),
                    with(Linetype::ByBlock, 1),
                ),
            ],
        )
        .unwrap();
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        let solid = RenderScene::PATTERN_CONTINUOUS;
        assert_eq!(patterns_of(scene), vec![solid, 1, 2, 1, solid, solid]);
        let rows = pattern_rows(scene);
        assert_eq!(rows.len(), 3, "continuous, then each pattern in use once");
        assert_eq!(rows[0], vec![0.0; RenderScene::FLOATS_PER_PATTERN]);
        assert_eq!(
            rows[1],
            vec![0.75, 2.0, 0.5, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            rows[2],
            vec![0.375, 2.0, 0.25, 0.125, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(scene.pattern_count(), 3);

        // LTSCALE multiplies the table and rebuilds no geometry.
        let mut tx = drawing.transaction();
        tx.set_ltscale(4.0).unwrap();
        tx.commit();
        let scene = cache.scene(&drawing);
        assert_eq!(&pattern_rows(scene)[1][..4], &[3.0, 2.0, 2.0, 1.0]);
        assert_eq!(patterns_of(scene), vec![solid, 1, 2, 1, solid, solid]);
        assert_eq!(cache.build_count(), 1);
        assert_eq!(cache.style_build_count(), 2);

        // So does giving the layer another linetype.
        let id = drawing.layers().find("dashed").unwrap();
        let mut layer = drawing.layers().get(id).cloned().unwrap();
        layer.linetype = Linetype::CONTINUOUS;
        let mut tx = drawing.transaction();
        tx.replace_layer(id, layer).unwrap();
        tx.commit();
        let scene = cache.scene(&drawing);
        assert_eq!(patterns_of(scene), vec![solid, solid, 1, 2, solid, solid]);
        assert_eq!(pattern_rows(scene).len(), 3);
        assert_eq!(cache.build_count(), 1);
    }

    // ── text (#143, ADR-0016) ──

    #[test]
    fn a_text_is_its_glyph_strokes_drawn_continuous_whatever_its_layer_says() {
        use cad_document::{Linetype, LinetypeDef, TextEntity};

        let dashed = Layer {
            linetype: Linetype::Named(cad_document::LinetypeId::from_raw(1)),
            ..Layer::named("dashed")
        };
        let text = TextEntity::new(Point2::new(0.0, 0.0), 10.0, 0.0, "HI");
        let segments_in_text: usize = text.strokes().iter().map(|stroke| stroke.len() - 1).sum();
        let drawing = Drawing::from_parts(
            [dashed],
            [LinetypeDef::built_in_named("Dashed").unwrap()],
            [
                (Entity::Text(text), on(1)),
                (Entity::Line(line(0.0, 20.0, 50.0, 20.0)), on(1)),
            ],
        )
        .unwrap();
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        assert_eq!(scene.frame().segment_count() as usize, segments_in_text + 1);
        assert_eq!(scene.frame().arc_count(), 0);
        // Two styles: the text's, continuous, and the line's, dashed — on
        // the same layer with the same properties otherwise.
        assert_eq!(scene.style_count(), 2);
        let patterns = patterns_of(scene);
        let text_style = scene.segment_styles()[0] as usize;
        let line_style = *scene.segment_styles().last().unwrap() as usize;
        assert_eq!(patterns[text_style], RenderScene::PATTERN_CONTINUOUS);
        assert_ne!(patterns[line_style], RenderScene::PATTERN_CONTINUOUS);
        assert!(
            scene.frame().segment_distances().iter().all(|d| *d == 0.0),
            "no path runs through a text"
        );
    }
}
