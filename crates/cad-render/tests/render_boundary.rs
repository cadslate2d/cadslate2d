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
        let drawing = Drawing::from_parts(layers, entities(LayerId::from_raw(1))).unwrap();
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
    assert_eq!(
        scene.styles(),
        &[
            0,
            0,
            0,
            RenderScene::STYLE_FOREGROUND,
            255,
            0,
            0,
            RenderScene::STYLE_RGB,
            0,
            0,
            255,
            RenderScene::STYLE_RGB,
        ]
    );

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
    assert_eq!(&scene.styles()[4..8], &[0, 255, 0, RenderScene::STYLE_RGB]);
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
        let (rows, _) = scene.styles().as_chunks::<4>();
        scene
            .segment_styles()
            .iter()
            .map(|style| rows[usize::from(*style)])
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
