//! The render-boundary suite (DESIGN §41, "Render boundary").
//!
//! One named test per rule in the design document, driven through the same
//! `Session` API the browser uses. If the boundary erodes, this is the file
//! that fails. The TypeScript half of the suite — the exported WASM API
//! snapshot and the calls-per-frame check — lives in
//! `web/src/cad/render-boundary.test.ts`.

use cad_commands::testing::Harness;
use cad_commands::{Dirty, Modifiers, PointerButton, Toggle};
use cad_core::ScreenPoint;
use cad_render::{OverlayFrame, PackedFrame, RenderScene, SceneCache, StyleRole};

/// A small drawing made the way a user would make it.
fn drawing() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", "100,50", "0,50", "0,0", ""]);
    h.enter_all(&["L", "-40,-40", "@200<30", ""]);
    h
}

/// The same drawing, mid-command, with the pointer on the canvas and one
/// entity selected beforehand.
fn interacting() -> Harness {
    let mut h = drawing();
    h.click(450.0, 300.0); // selects the bottom edge
    h.enter_all(&["L", "10,10"]);
    h.move_to(520.0, 240.0);
    h
}

fn overlay_of(h: &Harness) -> OverlayFrame {
    let mut overlay = OverlayFrame::new();
    overlay.rebuild(&h.session.overlay(), h.session.drawing(), h.session.view());
    overlay
}

fn assert_frame_is_consistent(frame: &PackedFrame, what: &str) {
    assert_eq!(
        frame.segments().len(),
        frame.segment_count() as usize * PackedFrame::FLOATS_PER_SEGMENT,
        "{what}: segment buffer length"
    );
    assert_eq!(
        frame.batch_words().len(),
        frame.batches().len() * PackedFrame::WORDS_PER_BATCH,
        "{what}: batch words"
    );

    let mut next = 0;
    for batch in frame.batches() {
        assert!(batch.count > 0, "{what}: empty batch");
        assert_eq!(
            batch.first, next,
            "{what}: batches must tile the buffer without gaps or overlap"
        );
        next = batch.first + batch.count;
    }
    assert_eq!(
        next,
        frame.segment_count(),
        "{what}: batches must cover every segment"
    );
    assert!(
        frame.segments().iter().all(|value| value.is_finite()),
        "{what}: non-finite coordinate"
    );
}

/// "deterministic committed RenderScene generation from deterministic
/// drawing/view state"
#[test]
fn committed_scene_is_deterministic() {
    let (a, b) = (drawing(), drawing());
    let scene_a = SceneCache::new().scene(a.session.drawing()).frame().clone();
    let scene_b = SceneCache::new().scene(b.session.drawing()).frame().clone();
    assert_eq!(scene_a, scene_b);
    assert_eq!(scene_a.segment_count(), 5);

    // The scene has no view input at all: zooming and panning change nothing.
    let mut moved = drawing();
    moved
        .session
        .wheel(ScreenPoint::new(50.0, 50.0), -400.0, Modifiers::NONE);
    moved.session.pointer_down(
        ScreenPoint::new(0.0, 0.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    moved
        .session
        .pointer_move(ScreenPoint::new(300.0, 200.0), Modifiers::NONE);
    assert_eq!(
        SceneCache::new().scene(moved.session.drawing()).frame(),
        &scene_a
    );
}

/// "deterministic OverlayFrame generation from deterministic transient state"
#[test]
fn overlay_frame_is_deterministic() {
    assert_eq!(overlay_of(&interacting()), overlay_of(&interacting()));
    assert!(overlay_of(&interacting()).frame().segment_count() > 0);
}

/// "pointer-only transient updates do not change the committed scene revision"
#[test]
fn pointer_only_updates_do_not_change_the_scene_revision() {
    let mut h = interacting();
    let mut scenes = SceneCache::new();
    let revision = scenes.scene(h.session.drawing()).revision();

    for step in 0..500 {
        let dirty = h.move_to(f64::from(step * 3 % 800), f64::from(step * 7 % 600));
        assert!(dirty.contains(Dirty::OVERLAY));
        assert!(
            !dirty.contains(Dirty::SCENE),
            "a pointer move must never flag the scene"
        );
        assert_eq!(scenes.scene(h.session.drawing()).revision(), revision);
    }
    // Selecting, cancelling, zooming, and leaving are transient too.
    h.escape();
    h.click(450.0, 300.0);
    h.session
        .wheel(ScreenPoint::new(400.0, 300.0), 120.0, Modifiers::NONE);
    h.session.pointer_leave();

    assert_eq!(scenes.scene(h.session.drawing()).revision(), revision);
    assert_eq!(
        scenes.build_count(),
        1,
        "the committed scene was built once and never again"
    );
}

/// "no entity-by-entity JS/WASM getter loop": frames are produced whole, in
/// one call each, whatever the entity count.
/// #80: showing a snap marker is overlay work only — the committed scene is
/// neither rebuilt nor given a new revision.
#[test]
fn snap_markers_cause_no_committed_upload() {
    let mut h = drawing();
    h.enter("L");
    let mut cache = SceneCache::new();
    let revision = cache.scene(h.session.drawing()).revision();
    let builds = cache.build_count();

    let mut markers = 0;
    for step in 0..200 {
        // Sweep across the whole view, so the cursor crosses snap points.
        let (x, y) = (f64::from(step) * 4.0, f64::from(step) * 3.0);
        let dirty = h.move_to(x, y);
        assert!(!dirty.contains(Dirty::SCENE));
        let overlay = overlay_of(&h);
        let has_marker = [overlay.frame().batches(), overlay.frame().arc_batches()]
            .concat()
            .iter()
            .any(|batch| batch.style == StyleRole::SnapMarker);
        markers += usize::from(has_marker);
        assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    }
    assert!(markers > 0, "the sweep never met a snap point");
    assert_eq!(cache.build_count(), builds, "the scene was rebuilt");
}

/// #118: hover feedback and pick previews are overlay work only. Hovering
/// objects while a command asks for one changes the overlay and never the
/// committed scene.
#[test]
fn hovering_objects_for_a_pick_causes_no_committed_upload() {
    let mut h = drawing();
    h.enter("TR");
    h.click(450.0, 300.0); // the bottom edge of the rectangle is the cutting edge
    h.enter("");
    let mut cache = SceneCache::new();
    let revision = cache.scene(h.session.drawing()).revision();
    let builds = cache.build_count();

    let (mut hovers, mut removals) = (0, 0);
    for step in 0..200 {
        // Straight across at world y = 20: over the rectangle's sides and
        // the slanted line, which crosses the cutting edge.
        let (x, y) = (350.0 + f64::from(step), 280.0);
        let dirty = h.move_to(x, y);
        assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
        let overlay = overlay_of(&h);
        assert_frame_is_consistent(overlay.frame(), "hover overlay");
        let has = |style: StyleRole| {
            overlay
                .frame()
                .batches()
                .iter()
                .any(|batch| batch.style == style)
        };
        hovers += usize::from(has(StyleRole::Hover));
        removals += usize::from(has(StyleRole::Removal));
        assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    }
    assert!(hovers > 0, "the sweep never crossed an object");
    assert!(removals > 0, "the sweep never previewed a trim");
    assert_eq!(cache.build_count(), builds, "the scene was rebuilt");
}

/// #82: the grid is view-dependent overlay geometry. Showing it, and
/// panning and zooming under it, never rebuilds the committed scene.
#[test]
fn the_grid_costs_no_committed_upload() {
    let mut h = drawing();
    let mut cache = SceneCache::new();
    let revision = cache.scene(h.session.drawing()).revision();
    let builds = cache.build_count();

    let dirty = h.toggle(Toggle::Grid);
    assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
    let mut counts = Vec::new();
    for step in 0..30 {
        let at = ScreenPoint::new(100.0 + f64::from(step) * 20.0, 300.0);
        let delta = if step % 3 == 0 { -240.0 } else { 90.0 };
        let dirty = h.session.wheel(at, delta, Modifiers::NONE);
        assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
        let overlay = overlay_of(&h);
        assert_frame_is_consistent(overlay.frame(), "grid overlay");
        counts.push(overlay.frame().segment_count());
        assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    }
    assert!(counts.iter().all(|count| *count > 0), "the grid vanished");
    assert_eq!(cache.build_count(), builds, "the scene was rebuilt");
}

#[test]
fn frames_are_produced_in_bulk() {
    let mut h = Harness::new();
    h.enter("L");
    for index in 0..300 {
        h.enter(&format!("{},{}", index * 3, index % 7));
    }
    h.enter("");

    let mut scenes = SceneCache::new();
    let scene = scenes.scene(h.session.drawing());
    assert_eq!(scene.frame().segment_count(), 299);
    assert_eq!(
        scene.frame().batches().len(),
        1,
        "299 entities, one batch, one buffer"
    );
    assert_eq!(scenes.build_count(), 1);
}

/// "buffer lengths and batch ranges"
#[test]
fn buffer_lengths_and_batch_ranges_are_consistent() {
    for (name, h) in [
        ("empty", Harness::new()),
        ("drawing", drawing()),
        ("interacting", interacting()),
    ] {
        assert_frame_is_consistent(
            SceneCache::new().scene(h.session.drawing()).frame(),
            &format!("{name} scene"),
        );
        assert_frame_is_consistent(overlay_of(&h).frame(), &format!("{name} overlay"));
    }
}

/// "renderer does not mutate CAD state"
#[test]
fn render_preparation_does_not_mutate_cad_state() {
    let h = interacting();
    let revision = h.session.drawing().revision();
    let next_id = h.session.drawing().next_entity_id();
    let ui = h.session.ui_state();
    let overlay_description = h.session.overlay();
    let selection: Vec<_> = h.session.selection().iter().collect();

    let mut scenes = SceneCache::new();
    let mut overlay = OverlayFrame::new();
    for _ in 0..50 {
        scenes.scene(h.session.drawing());
        overlay.rebuild(&h.session.overlay(), h.session.drawing(), h.session.view());
    }

    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.session.drawing().next_entity_id(), next_id);
    assert_eq!(h.session.ui_state(), ui);
    assert_eq!(h.session.overlay(), overlay_description);
    assert_eq!(h.session.selection().iter().collect::<Vec<_>>(), selection);
}

/// #132: objects on a layer that is off are not in the scene at all — not
/// drawn dimmed, not drawn and discarded: no instances.
#[test]
fn objects_on_a_layer_that_is_off_are_not_in_the_scene() {
    use cad_core::Point2;
    use cad_document::{Drawing, Entity, Layer, LayerId, LineEntity, Properties};

    let line =
        |y: f64| -> Entity { LineEntity::new(Point2::new(0.0, y), Point2::new(100.0, y)).into() };
    let entities = |hidden: LayerId| {
        [
            (line(0.0), Properties::default()),
            (line(10.0), Properties::on(hidden)),
            (line(20.0), Properties::on(LayerId::from_raw(2))),
        ]
    };
    let build = |off: bool| {
        let layers = [
            Layer {
                on: !off,
                ..Layer::named("maybe hidden")
            },
            Layer {
                locked: true,
                ..Layer::named("locked")
            },
        ];
        let drawing = Drawing::from_parts(layers, [], entities(LayerId::from_raw(1))).unwrap();
        let mut cache = SceneCache::default();
        let scene = cache.scene(&drawing);
        assert_frame_is_consistent(scene.frame(), "scene");
        scene.frame().segment_count()
    };
    assert_eq!(build(false), 3, "everything on, locked or not");
    assert_eq!(build(true), 2, "the layer that is off contributes nothing");
}

/// A drawing with layers in three colours, made the way a user makes it.
fn coloured() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&[
        "LA",
        "N",
        "walls,doors",
        "C",
        "1",
        "walls",
        "C",
        "5",
        "doors",
        "",
    ]);
    h.enter_all(&["L", "0,0", "100,0", "100,50", ""]);
    h.enter_all(&[
        "LA", "S", "walls", "", "C", "50,25", "10", "L", "0,60", "90,60", "",
    ]);
    h.enter_all(&[
        "LA", "S", "doors", "", "PL", "0,-20", "40,-20", "A", "40,-40", "",
    ]);
    h
}

/// #134 (ADR-0014): with several colours, the committed scene still crosses
/// as a few whole buffers — one style index per instance, every index a row
/// of the table — and is the same every time it is built.
#[test]
fn styled_scenes_are_consistent_and_deterministic() {
    let h = coloured();
    let mut cache = SceneCache::new();
    let scene = cache.scene(h.session.drawing());
    assert_frame_is_consistent(scene.frame(), "scene");

    assert_eq!(
        scene.segment_styles().len(),
        scene.frame().segment_count() as usize
    );
    assert_eq!(scene.arc_styles().len(), scene.frame().arc_count() as usize);
    assert_eq!(
        scene.styles().len(),
        scene.style_count() * RenderScene::BYTES_PER_STYLE
    );
    assert_eq!(scene.style_count(), 3, "layer 0, walls, and doors");
    for style in scene.segment_styles().iter().chain(scene.arc_styles()) {
        assert!(usize::from(*style) < scene.style_count());
    }
    // Committed geometry is still one batch per buffer, however many colours:
    // colour is not what batches are for any more.
    assert_eq!(scene.frame().batches().len(), 1);
    assert_eq!(scene.frame().arc_batches().len(), 1);
    // Layer 0 is the foreground; walls red; doors blue.
    let thin = RenderScene::STYLE_WEIGHT_DEFAULT;
    #[rustfmt::skip]
    let expected = [
        0, 0, 0, RenderScene::STYLE_FOREGROUND, thin, 0, 0, 0,
        255, 0, 0, RenderScene::STYLE_RGB, thin, 0, 0, 0,
        0, 0, 255, RenderScene::STYLE_RGB, thin, 0, 0, 0,
    ];
    assert_eq!(scene.styles(), &expected);

    let again = coloured();
    let mut other = SceneCache::new();
    let rebuilt = other.scene(again.session.drawing());
    assert_eq!(rebuilt.frame(), scene.frame());
    assert_eq!(rebuilt.segment_styles(), scene.segment_styles());
    assert_eq!(rebuilt.arc_styles(), scene.arc_styles());
    assert_eq!(rebuilt.styles(), scene.styles());
}

/// #134: pointer movement, pan, and zoom build neither geometry nor styles;
/// recolouring a layer resolves the styles and builds no geometry.
#[test]
fn pointer_activity_touches_neither_geometry_nor_styles_and_a_recolour_only_styles() {
    let mut h = coloured();
    let mut cache = SceneCache::new();
    let (geometry, styles) = {
        let scene = cache.scene(h.session.drawing());
        (scene.revision(), scene.style_revision())
    };

    h.enter("L");
    for step in 0..200 {
        h.move_to(f64::from(step * 3 % 800), f64::from(step * 7 % 600));
        let scene = cache.scene(h.session.drawing());
        assert_eq!(
            (scene.revision(), scene.style_revision()),
            (geometry, styles)
        );
    }
    h.escape();
    h.session
        .wheel(ScreenPoint::new(400.0, 300.0), 120.0, Modifiers::NONE);
    h.session.pointer_down(
        ScreenPoint::new(100.0, 100.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    h.move_to(180.0, 140.0);
    h.session.pointer_up(
        ScreenPoint::new(180.0, 140.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    cache.scene(h.session.drawing());
    assert_eq!((cache.build_count(), cache.style_build_count()), (1, 1));

    let dirty = h.enter_all_dirty(&["LA", "C", "3", "walls", ""]);
    assert!(dirty.contains(Dirty::SCENE), "the host is told to look");
    let scene = cache.scene(h.session.drawing());
    assert_eq!(scene.revision(), geometry, "the geometry is untouched");
    assert_ne!(scene.style_revision(), styles);
    assert_eq!(&scene.styles()[8..12], &[0, 255, 0, RenderScene::STYLE_RGB]);
    assert_eq!((cache.build_count(), cache.style_build_count()), (1, 2));
}

/// #135: an object with a colour of its own is a style of its own, and a
/// change of its layer's colour leaves it as it is.
#[test]
fn an_objects_own_colour_wins_over_its_layers_on_screen() {
    let mut h = Harness::new();
    h.enter_all(&["LA", "N", "walls", "C", "blue", "walls", "S", "walls", ""]);
    h.enter_all(&["L", "0,0", "10,0", ""]); // ByLayer
    h.enter_all(&["COLOR", "red", "L", "0,5", "10,5", ""]); // its own red

    // The row each line is drawn with, in the order the lines were drawn.
    let rows = |h: &Harness| -> Vec<[u8; 4]> {
        let mut cache = SceneCache::new();
        let scene = cache.scene(h.session.drawing());
        let (rows, _) = scene
            .styles()
            .as_chunks::<{ RenderScene::BYTES_PER_STYLE }>();
        scene
            .segment_styles()
            .iter()
            .map(|style| rows[usize::from(*style)])
            .map(|row| [row[0], row[1], row[2], row[3]])
            .collect()
    };
    let rgb = |red, green, blue| [red, green, blue, RenderScene::STYLE_RGB];
    assert_eq!(rows(&h), vec![rgb(0, 0, 255), rgb(255, 0, 0)]);

    h.enter_all(&["LA", "C", "green", "walls", ""]);
    assert_eq!(
        rows(&h),
        vec![rgb(0, 255, 0), rgb(255, 0, 0)],
        "only the ByLayer line followed its layer"
    );
}

// ── block references (#147, ADR-0017) ──

mod blocks {
    use cad_commands::testing::Harness;
    use cad_core::Point2;
    use cad_document::{
        Block, BlockId, BlockReferenceEntity, Color, Drawing, Entity, EntityRecord, Layer, LayerId,
        LineEntity, Lineweight, Properties,
    };
    use cad_render::SceneCache;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        Entity::Line(LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    }

    fn by_block() -> Properties {
        Properties {
            color: Color::ByBlock,
            lineweight: Lineweight::ByBlock,
            ..Properties::default()
        }
    }

    /// A chair (two lines, one on layer 0 ByBlock, one on `fixed` in red),
    /// a pair of chairs, and two pairs placed: eight lines on screen.
    fn nested() -> (Drawing, BlockId, BlockId) {
        let mut drawing = Drawing::from_parts(
            [Layer {
                color: Color::Index(5),
                ..Layer::named("fixed")
            }],
            [],
            [],
        )
        .unwrap();
        let mut tx = drawing.transaction();
        let chair = tx
            .define_block(Block::new(
                "Chair",
                Point2::ORIGIN,
                vec![
                    EntityRecord::new(line(0.0, 0.0, 10.0, 0.0), by_block()),
                    EntityRecord::new(
                        line(0.0, 0.0, 0.0, 10.0),
                        Properties {
                            color: Color::Index(1),
                            ..Properties::on(LayerId::from_raw(1))
                        },
                    ),
                ],
            ))
            .unwrap();
        let pair = tx
            .define_block(Block::new(
                "Pair",
                Point2::ORIGIN,
                // The inner references are ByBlock themselves, so what is
                // ByBlock in a chair reaches the outer reference.
                vec![
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::ORIGIN, 1.0, 0.0),
                        by_block(),
                    ),
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::new(20.0, 0.0), 1.0, 0.0),
                        by_block(),
                    ),
                ],
            ))
            .unwrap();
        tx.insert(BlockReferenceEntity::new(
            pair,
            Point2::new(100.0, 0.0),
            1.0,
            0.0,
        ))
        .unwrap();
        tx.insert_with(
            BlockReferenceEntity::new(pair, Point2::new(100.0, 100.0), 2.0, 0.0),
            Properties {
                color: Color::Index(3),
                lineweight: Lineweight::Hundredths(50),
                ..Properties::default()
            },
        )
        .unwrap();
        tx.commit();
        (drawing, chair, pair)
    }

    #[test]
    fn a_reference_is_drawn_as_its_definitions_geometry_nested_and_transformed() {
        let (drawing, ..) = nested();
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        let frame = scene.frame();
        assert_eq!(
            frame.segment_count(),
            8,
            "two pairs of two chairs of two lines"
        );
        // The second pair is scaled by two: its chairs' lines are 20 long,
        // from (100,100) and (140,100) — the inner reference's offset of 20
        // is scaled as well.
        let segments = frame.segments();
        let origin = frame.origin();
        let world = |i: usize| {
            (
                f64::from(segments[i * 4]) + origin.x,
                f64::from(segments[i * 4 + 1]) + origin.y,
                f64::from(segments[i * 4 + 2]) + origin.x,
                f64::from(segments[i * 4 + 3]) + origin.y,
            )
        };
        assert_eq!(world(0), (100.0, 0.0, 110.0, 0.0));
        assert_eq!(world(6), (140.0, 100.0, 160.0, 100.0));
        assert_eq!(world(7), (140.0, 100.0, 140.0, 120.0));
        // Determinism: the same drawing yields the same bytes.
        let again = SceneCache::new().scene(&drawing).clone();
        assert_eq!(&again, scene);
    }

    #[test]
    fn by_block_takes_the_references_own_and_each_result_is_a_style() {
        let (drawing, ..) = nested();
        let mut cache = SceneCache::new();
        let scene = cache.scene(&drawing);
        let (rows, _) = scene.styles().as_chunks::<8>();
        let rows: Vec<[u8; 5]> = rows
            .iter()
            .map(|row| [row[0], row[1], row[2], row[3], row[4]])
            .collect();
        let style_of = |segment: usize| rows[scene.segment_styles()[segment] as usize];
        // First pair, plain: the ByBlock line is ByLayer of the reference,
        // on layer 0 — the foreground; the red line is red.
        assert_eq!(style_of(0)[3], 0, "foreground");
        assert_eq!(&style_of(1)[..4], &[255, 0, 0, 255]);
        // Second pair, green and heavy: the ByBlock line is green and 0.50 mm;
        // the red line keeps its red and takes its layer's default weight.
        assert_eq!(&style_of(6)[..4], &[0, 255, 0, 255]);
        assert_eq!(style_of(6)[4], 50);
        assert_eq!(&style_of(7)[..4], &[255, 0, 0, 255]);
        assert_eq!(style_of(7)[4], 255);
    }

    #[test]
    fn a_redefinition_changes_the_scene_revision_once_and_every_reference_follows() {
        let (mut drawing, chair, _) = nested();
        let mut cache = SceneCache::new();
        let before = cache.scene(&drawing).revision();
        let mut tx = drawing.transaction();
        tx.redefine_block(
            chair,
            Block::new(
                "Chair",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    line(0.0, 0.0, 5.0, 5.0),
                    Properties::default(),
                )],
            ),
        )
        .unwrap();
        tx.commit();
        let scene = cache.scene(&drawing);
        let (revision, segments) = (scene.revision(), scene.frame().segment_count());
        assert_ne!(revision, before, "what is shown changed");
        assert_eq!(segments, 4, "one line per chair now");
        assert_eq!(cache.build_count(), 2, "one rebuild for the one change");
        cache.scene(&drawing);
        assert_eq!(cache.build_count(), 2, "and no more");
        drawing.undo();
        assert_ne!(cache.scene(&drawing).revision(), revision);
        assert_eq!(cache.scene(&drawing).frame().segment_count(), 8);
    }

    #[test]
    fn a_selected_reference_is_highlighted_whole_in_the_overlay() {
        let (drawing, ..) = nested();
        let mut h = Harness::with_drawing(drawing);
        h.click_world(Point2::new(105.0, 0.0));
        assert_eq!(h.session.selection().len(), 1);
        let overlay = super::overlay_of(&h);
        let highlighted: u32 = overlay
            .frame()
            .batches()
            .iter()
            .filter(|batch| batch.style == cad_render::StyleRole::Selection)
            .map(|batch| batch.count)
            .sum();
        assert_eq!(highlighted, 4, "the whole pair, both chairs");
    }
}

/// DESIGN §34 (#161): what the renderer draws for a text is inside the box
/// the document computes for it — at every justification and angle, after
/// the float conversion the scene makes.
#[test]
fn the_scenes_text_segments_lie_inside_the_documents_text_box() {
    use cad_document::{Drawing, Entity, Justification, TextEntity};
    use cad_render::{PackedFrame, SceneCache};

    let mut texts = Vec::new();
    for (index, justification) in [
        Justification::Left,
        Justification::Center,
        Justification::Right,
    ]
    .into_iter()
    .enumerate()
    {
        for step in 0..8 {
            let angle = f64::from(step) * std::f64::consts::FRAC_PI_4 + 0.3;
            let at = cad_core::Point2::new(f64::from(step) * 300.0, index as f64 * 200.0 - 50.0);
            texts.push(
                TextEntity::new(at, 7.5 + f64::from(step), angle, "Slate [12.5] gy")
                    .justified(justification),
            );
        }
    }
    let drawing = Drawing::from_entities(texts.iter().cloned().map(Entity::Text));
    let mut cache = SceneCache::new();
    let frame = cache.scene(&drawing).frame();
    let (segments, _) = frame
        .segments()
        .as_chunks::<{ PackedFrame::FLOATS_PER_SEGMENT }>();
    assert!(!segments.is_empty());
    let boxes: Vec<_> = texts.iter().map(TextEntity::frame).collect();
    // Segments are offsets from the frame's f64 origin (ADR-0003).
    let origin = frame.origin();
    let inside = |x: f32, y: f32| {
        let point = origin + cad_core::Vec2::new(f64::from(x), f64::from(y));
        boxes.iter().any(|frame| {
            let local = point - frame.start;
            let along = local.dot(frame.along);
            let up = local.dot(frame.up);
            // Single precision at coordinates in the thousands: a thousandth.
            let slack = 1e-3;
            along >= -slack
                && along <= frame.width + slack
                && up >= -frame.descent - slack
                && up <= frame.ascent + slack
        })
    };
    for segment in segments {
        assert!(inside(segment[0], segment[1]), "{segment:?}");
        assert!(inside(segment[2], segment[3]), "{segment:?}");
    }
}
