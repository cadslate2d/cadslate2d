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

use std::collections::HashMap;

use cad_core::Point2;
use cad_document::{
    Drawing, Entity, Linetype, LinetypeDef, LinetypeId, Paint, Properties, Revision, Weight,
};

use crate::{PackedFrame, StyleRole};

/// Stable geometry derived from the canonical document.
///
/// A scene is a function of the drawing alone — there is no view parameter —
/// so pan, zoom, and pointer movement can never invalidate it (ADR-0003).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderScene {
    revision: Revision,
    frame: PackedFrame,
    /// The style index of each segment instance, in instance order.
    segment_styles: Vec<u16>,
    /// The style index of each arc instance, in instance order.
    arc_styles: Vec<u16>,
    /// The properties each style index stands for.
    properties: Vec<Properties>,
    style_revision: Revision,
    /// The style table as it crosses the boundary: see [`Self::styles`].
    styles: Vec<u8>,
    /// The pattern table: see [`Self::patterns`].
    patterns: Vec<f32>,
}

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

    fn build_geometry(&mut self, drawing: &Drawing) {
        // Centre the origin on the drawing so f32 offsets keep the most
        // precision where the geometry actually is.
        let origin = drawing
            .extents()
            .map_or(Point2::ORIGIN, |extents| extents.center());
        self.frame.reset(origin);
        self.segment_styles.clear();
        self.arc_styles.clear();
        self.properties.clear();

        let mut indices: HashMap<Properties, u16> = HashMap::new();
        // Objects on a layer that is off are not drawn: no instances.
        for (id, entity) in drawing.shown() {
            let mut properties = drawing.properties(id).unwrap_or_default();
            // A linetype is for curves: a text is drawn continuous, or its
            // glyphs would fall apart into dashes.
            if matches!(entity, Entity::Text(_) | Entity::MText(_)) {
                properties.linetype = Linetype::CONTINUOUS;
            }
            let style = *indices.entry(properties).or_insert_with(|| {
                // More distinct styles than indices: the surplus share the
                // last one rather than wrapping round onto the first.
                let index = self.properties.len().min(usize::from(u16::MAX));
                if index == self.properties.len() {
                    self.properties.push(properties);
                }
                index as u16
            });
            let (segments, arcs) = (self.frame.segment_count(), self.frame.arc_count());
            self.frame.push_entity(StyleRole::Entity, entity);
            let added = |before: u32, after: u32| (after - before) as usize;
            self.segment_styles.extend(std::iter::repeat_n(
                style,
                added(segments, self.frame.segment_count()),
            ));
            self.arc_styles.extend(std::iter::repeat_n(
                style,
                added(arcs, self.frame.arc_count()),
            ));
        }
        self.revision = drawing.shown_revision();
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
/// part depends on has changed since the last request.
#[derive(Debug, Default)]
pub struct SceneCache {
    scene: Option<RenderScene>,
    builds: u64,
    style_builds: u64,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The scene for `drawing`. The geometry is rebuilt if and only if what
    /// is shown changed; the style table is re-resolved if and only if the
    /// drawing changed at all.
    pub fn scene(&mut self, drawing: &Drawing) -> &RenderScene {
        let stale = |scene: &RenderScene| scene.revision != drawing.shown_revision();
        if self.scene.as_ref().is_none_or(stale) {
            // The previous scene's buffers are reused, to avoid reallocating.
            let mut scene = self.scene.take().unwrap_or_else(|| RenderScene {
                revision: drawing.shown_revision(),
                frame: PackedFrame::default(),
                segment_styles: Vec::new(),
                arc_styles: Vec::new(),
                properties: Vec::new(),
                style_revision: drawing.revision(),
                styles: Vec::new(),
                patterns: Vec::new(),
            });
            scene.build_geometry(drawing);
            self.builds += 1;
            // New geometry may have new styles: resolve them whatever the
            // drawing's revision says.
            scene.resolve_styles(drawing);
            self.style_builds += 1;
            self.scene = Some(scene);
        }
        let scene = self.scene.as_mut().expect("the scene was just built");
        if scene.style_revision != drawing.revision() {
            scene.resolve_styles(drawing);
            self.style_builds += 1;
        }
        scene
    }

    /// How many times the geometry has been built. Tests use this to prove
    /// that pointer activity — and a change to how a layer looks — does not
    /// regenerate committed geometry.
    pub fn build_count(&self) -> u64 {
        self.builds
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
        assert_eq!(rows(scene), vec![RED, BLUE, BLUE]);
        assert_eq!(scene.segment_styles(), &[0, 1, 0, 0, 2]);
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
