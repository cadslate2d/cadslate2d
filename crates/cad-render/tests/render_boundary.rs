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
use cad_render::{InstanceKind, OverlayFrame, PackedFrame, RenderScene, SceneCache, StyleRole};

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

/// #185: a fence or polygon being drawn is overlay work only — its
/// segments are in the overlay, and the committed scene is neither rebuilt
/// nor given a new revision.
#[test]
fn a_fence_being_drawn_causes_no_committed_upload() {
    let mut h = drawing();
    let mut cache = SceneCache::new();
    let revision = cache.scene(h.session.drawing()).revision();
    let builds = cache.build_count();

    h.enter_all(&["E", "F"]);
    h.click(100.0, 100.0);
    h.click(300.0, 400.0);
    let dirty = h.move_to(500.0, 200.0);
    assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
    let overlay = overlay_of(&h);
    let fence_segments: usize = overlay
        .frame()
        .batches()
        .iter()
        .filter(|batch| batch.style == StyleRole::SelectionCrossing)
        .map(|batch| batch.count as usize)
        .sum();
    assert_eq!(fence_segments, 2, "two points and the rubber band");
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    assert_eq!(cache.build_count(), builds, "the scene was rebuilt");

    h.enter_all(&["", "WP"]); // Enter ends the fence; the polygon starts
    h.click(100.0, 100.0);
    h.click(300.0, 100.0);
    h.move_to(300.0, 300.0);
    let overlay = overlay_of(&h);
    let polygon_segments: usize = overlay
        .frame()
        .batches()
        .iter()
        .filter(|batch| batch.style == StyleRole::SelectionWindow)
        .map(|batch| batch.count as usize)
        .sum();
    assert_eq!(polygon_segments, 3, "two edges and the closing edge");
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
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

/// #188: the regions tile every buffer — each kind's runs start where the
/// last ended and reach its end — and a drawing of ordinary size is one
/// region, so the adapter draws it as it did. The transforms of a frame
/// are one per region, and pointer movement changes none of the geometry.
#[test]
fn regions_tile_the_buffers_and_an_ordinary_drawing_is_one() {
    for (name, h) in [
        ("empty", Harness::new()),
        ("drawing", drawing()),
        ("interacting", interacting()),
    ] {
        let mut cache = SceneCache::new();
        let scene = cache.scene(h.session.drawing());
        let regions = scene.regions();
        assert_eq!(regions.len(), 1, "{name}: one region");
        let counts = [
            scene.frame().segment_count(),
            scene.frame().arc_count(),
            scene.frame().triangle_count(),
        ];
        assert_eq!(regions[0].first, [0, 0, 0], "{name}");
        assert_eq!(regions[0].count, counts, "{name}");
    }
    // A drawing spanning ten million units: two regions, tiling each kind.
    let mut h = Harness::new();
    // IDs start at 1: this many lines fill the first chunk exactly.
    let mut lines = Vec::new();
    for index in 1..cad_render::CHUNK_IDS {
        let y = index as f64;
        lines.push(cad_document::Entity::from(cad_document::LineEntity::new(
            cad_core::Point2::new(0.0, y),
            cad_core::Point2::new(10.0, y),
        )));
    }
    let far = 1e7;
    lines.push(cad_document::CircleEntity::new(cad_core::Point2::new(far, far), 30.0).into());
    lines.push(
        cad_document::LineEntity::new(
            cad_core::Point2::new(far, far),
            cad_core::Point2::new(far + 100.0, far),
        )
        .into(),
    );
    h.session.open_drawing(
        cad_document::Drawing::from_entities(lines),
        "span.dxf",
        vec![],
        vec![],
    );
    let mut cache = SceneCache::new();
    let scene = cache.scene(h.session.drawing());
    let regions = scene.regions();
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].first, [0, 0, 0]);
    assert_eq!(regions[1].first, regions[0].count);
    let counts = [
        scene.frame().segment_count(),
        scene.frame().arc_count(),
        scene.frame().triangle_count(),
    ];
    let totals: Vec<u32> = regions[0]
        .count
        .iter()
        .zip(regions[1].count)
        .map(|(a, b)| a + b)
        .collect();
    assert_eq!(totals, counts);
    assert_eq!(regions[1].count, [1, 1, 0], "the far line and circle");
    assert!(
        regions[1]
            .origin
            .distance_to(cad_core::Point2::new(far + 35.0, far))
            < 1e-6
    );
    // The far chunk's circle is placed exactly: its centre is an offset of
    // tens of units from its own origin.
    let arcs = scene.frame().arcs();
    let at = regions[1].first[1] as usize * PackedFrame::FLOATS_PER_ARC;
    let centre =
        regions[1].origin + cad_core::Vec2::new(f64::from(arcs[at]), f64::from(arcs[at + 1]));
    assert!(
        centre.distance_to(cad_core::Point2::new(far, far)) < 1e-3,
        "{centre:?}"
    );
    let revision = scene.revision();
    h.move_to(10.0, 10.0);
    h.session
        .wheel(ScreenPoint::new(400.0, 300.0), 120.0, Modifiers::NONE);
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    assert_eq!(cache.build_count(), 1);
}

/// ADR-0022: an edit re-packs the chunk holding what it changed and
/// reports the instances to upload as ranges; pointer activity reports
/// nothing; a layer switched off rebuilds everything.
#[test]
fn an_edit_repacks_one_chunk_and_reports_the_ranges_it_changed() {
    let mut h = drawing();
    let mut cache = SceneCache::new();
    let before = cache.scene(h.session.drawing()).frame().clone();
    let whole = cache.take_dirty();
    assert_eq!(
        whole
            .iter()
            .map(|range| (range.kind, range.first, range.count))
            .collect::<Vec<_>>(),
        vec![
            (InstanceKind::Segments, 0, before.segment_count()),
            (InstanceKind::Arcs, 0, 0),
            (InstanceKind::Triangles, 0, 0),
        ],
        "the first build is everything"
    );
    assert!(cache.take_dirty().is_empty(), "taken once");

    h.move_to(300.0, 300.0);
    cache.scene(h.session.drawing());
    assert!(
        cache.take_dirty().is_empty(),
        "pointer movement changes nothing"
    );
    assert_eq!(cache.build_count(), 1);

    // One more line: its chunk is re-packed, and since it is the last
    // chunk, the range is the new segment alone.
    h.enter_all(&["L", "0,-20", "100,-20", ""]);
    let scene = cache.scene(h.session.drawing()).clone();
    assert_frame_is_consistent(scene.frame(), "after an edit");
    assert_eq!(scene.frame().segment_count(), before.segment_count() + 1);
    assert_eq!(
        &scene.frame().segments()[..before.segments().len()],
        before.segments(),
        "what was there is where it was, from the same origin"
    );
    assert_eq!(cache.partial_build_count(), 1);
    let ranges = cache.take_dirty();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].kind, InstanceKind::Segments);
    assert!(
        ranges[0].first <= before.segment_count() && ranges[0].count <= before.segment_count() + 1,
        "{ranges:?}: within the chunk"
    );
    let full = SceneCache::new().scene(h.session.drawing()).frame().clone();
    assert_eq!(
        scene.frame().segments().len(),
        full.segments().len(),
        "the repacked scene is as long as one built afresh"
    );

    // A layer switched off: the drawing cannot say what changed, so all of it.
    h.enter_all(&["LAYER", "N", "other", ""]);
    h.enter_all(&["LAYER", "OFF", "other", ""]);
    let count = cache.scene(h.session.drawing()).frame().segment_count();
    let ranges = cache.take_dirty();
    assert_eq!(ranges.len(), 3);
    assert_eq!((ranges[0].first, ranges[0].count), (0, count), "everything");
}

/// #226 (ADR-0032): an instance's style word carries its entity's tier in
/// the drawing order beside its style; a wipeout's mask is a background
/// row; a change of order re-packs only the chunk it touched, with no new
/// geometry, and the pointer moving re-packs nothing.
#[test]
fn the_drawing_order_crosses_in_the_style_words_and_repacks_only_what_moved() {
    let mut h = Harness::new();
    // A chunk's worth of lines, so the line moved below is not in the
    // last chunk.
    for n in 0..600 {
        h.enter_all(&["L", &format!("0,{n}"), &format!("100,{n}"), ""]);
    }
    h.enter_all(&["WIPEOUT", "10,10", "60,10", "60,30", ""]);
    let mut cache = SceneCache::new();
    let scene = cache.scene(h.session.drawing()).clone();
    cache.take_dirty();
    let tier = |word: u32| (word >> RenderScene::ORDER_SHIFT) as u16 as i16;
    let style = |word: u32| (word & 0xFFFF) as usize;
    assert!(
        scene.segment_styles()[..600]
            .iter()
            .all(|word| tier(*word) == 0)
    );
    // The wipeout: its mask in the background row, over everything; its
    // frame in its own style, in the same tier.
    let masks = scene.triangle_styles();
    assert_eq!(masks.len(), 1);
    assert_eq!(tier(masks[0]), 1);
    let row = style(masks[0]) * RenderScene::BYTES_PER_STYLE;
    assert_eq!(scene.styles()[row + 3], RenderScene::STYLE_BACKGROUND);
    let frame = &scene.segment_styles()[600..];
    assert_eq!(frame.len(), 3);
    assert!(
        frame
            .iter()
            .all(|word| tier(*word) == 1 && style(*word) == 0)
    );

    h.move_to(300.0, 300.0);
    cache.scene(h.session.drawing());
    assert!(
        cache.take_dirty().is_empty(),
        "pointer movement re-packs nothing"
    );

    // The fifth line to the front: one chunk, its segments' words alone.
    h.click_world(cad_core::Point2::new(50.0, 4.0));
    h.enter_all(&["DR", "F"]);
    let builds = cache.build_count();
    let after = cache.scene(h.session.drawing()).clone();
    assert_eq!(cache.build_count(), builds + 1);
    assert_eq!(cache.partial_build_count(), 1);
    assert_eq!(
        after.frame().segments(),
        scene.frame().segments(),
        "no new geometry"
    );
    assert_eq!(tier(after.segment_styles()[4]), 2);
    let changed: Vec<usize> = (0..after.segment_styles().len())
        .filter(|n| after.segment_styles()[*n] != scene.segment_styles()[*n])
        .collect();
    assert_eq!(changed, [4]);
    let ranges = cache.take_dirty();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].kind, InstanceKind::Segments);
    assert!(
        ranges[0].first == 0 && ranges[0].count as u64 == cad_render::CHUNK_IDS - 1,
        "{ranges:?}: the first chunk, IDs 1 to 511"
    );
}

/// #112: zooming far in on a curve draws its visible span in the overlay
/// and touches nothing committed.
#[test]
fn zooming_far_in_on_a_curve_uploads_nothing_committed() {
    let mut h = Harness::new();
    h.enter_all(&["C", "0,0", "100"]);
    let mut cache = SceneCache::new();
    let revision = cache.scene(h.session.drawing()).revision();
    assert_eq!(cache.scene(h.session.drawing()).frame().arc_count(), 1);
    // The rightmost point of the circle is at (500, 300) on screen; wheel
    // in there, e-fold a step, to a radius past a million pixels.
    let at = ScreenPoint::new(500.0, 300.0);
    for _ in 0..10 {
        let dirty = h.session.wheel(at, -500.0, Modifiers::NONE);
        assert!(!dirty.contains(Dirty::SCENE));
    }
    assert!(h.session.view().scale() * 100.0 > 1e6);
    let overlay = overlay_of(&h);
    assert!(
        overlay.frame().segment_count() > 0,
        "the span is in the overlay"
    );
    assert_frame_is_consistent(overlay.frame(), "far curve overlay");
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    assert_eq!(cache.build_count(), 1);
    assert!(
        cache.take_dirty().len() <= 3,
        "the first build's ranges, and no more"
    );
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
        assert!(((*style & 0xFFFF) as usize) < scene.style_count());
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
            .map(|style| rows[(*style & 0xFFFF) as usize])
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

/// ADR-0020: a dimension is drawn from its style — extension lines,
/// arrowheads, the arc, the strokes of its text — and drawn afresh when
/// the style changes; the pointer leaves the committed scene alone.
#[test]
fn a_dimension_is_drawn_by_its_style_and_redrawn_when_the_style_changes() {
    use cad_document::{DimensionEntity, Entity, Measure};
    let angle = DimensionEntity::new(
        Measure::Angular {
            vertex: cad_core::Point2::new(0.0, 0.0),
            a: cad_core::Point2::new(100.0, 0.0),
            b: cad_core::Point2::new(0.0, 100.0),
        },
        cad_core::Point2::new(60.0, 60.0),
    );
    let mut h = Harness::with_entities([Entity::from(angle)]);
    let mut cache = SceneCache::new();
    let (revision, segments, arcs) = {
        let scene = cache.scene(h.session.drawing());
        assert_frame_is_consistent(scene.frame(), "a dimension");
        (
            scene.revision(),
            scene.frame().segments().to_vec(),
            scene.frame().arc_count(),
        )
    };
    assert_eq!(arcs, 1, "the dimension arc");
    // Two extension lines, two arrowheads of three sides, and the text.
    assert!(segments.len() / PackedFrame::FLOATS_PER_SEGMENT > 8);

    h.move_to(100.0, 100.0);
    h.move_to(300.0, 200.0);
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);

    h.enter_all(&["DIMSTYLE", "H", "10", ""]);
    let scene = cache.scene(h.session.drawing());
    assert_ne!(
        scene.revision(),
        revision,
        "one row changed; the scene follows"
    );
    assert_frame_is_consistent(scene.frame(), "a dimension restyled");
    assert_ne!(
        scene.frame().segments(),
        &segments[..],
        "the text is drawn taller"
    );
    assert_eq!(scene.frame().arc_count(), 1);
    assert_eq!(
        cache.partial_build_count(),
        1,
        "the style change re-packed the chunk holding its dimension, not the scene (ADR-0022)"
    );
}

/// ADR-0033: a multileader is drawn from its style — its leaders, their
/// arrowheads, the dogleg, the strokes of its note — and drawn afresh when
/// the style changes, re-packing only the chunk that holds it.
#[test]
fn a_multileader_is_drawn_by_its_style_and_redrawn_when_the_style_changes() {
    use cad_core::Point2;
    use cad_document::{
        Drawing, Entity, MLeaderContent, MLeaderEntity, MLeaderStyle, MLeaderStyleId,
    };
    let mut mleader = MLeaderEntity::new(
        Point2::new(0.0, 0.0),
        Point2::new(20.0, 10.0),
        MLeaderContent::Text("Hole".to_owned()),
    );
    mleader
        .leaders
        .push(vec![Point2::new(5.0, -10.0), Point2::new(10.0, 0.0)]);
    let mut drawing = Drawing::from_entities([Entity::from(mleader)]);
    let mut cache = SceneCache::new();
    let (revision, segments) = {
        let scene = cache.scene(&drawing);
        assert_frame_is_consistent(scene.frame(), "a multileader");
        (scene.revision(), scene.frame().segments().to_vec())
    };
    // Three leader segments, the dogleg, two arrowheads of three sides,
    // and the note's strokes.
    assert!(segments.len() / PackedFrame::FLOATS_PER_SEGMENT > 10);
    assert_eq!(cache.scene(&drawing).revision(), revision);

    let mut tx = drawing.transaction();
    tx.replace_mleader_style(
        MLeaderStyleId::FIRST,
        MLeaderStyle {
            text_height: 5.0,
            ..MLeaderStyle::named(MLeaderStyle::STANDARD)
        },
    )
    .unwrap();
    tx.commit();
    let scene = cache.scene(&drawing);
    assert_ne!(
        scene.revision(),
        revision,
        "one row changed; the scene follows"
    );
    assert_frame_is_consistent(scene.frame(), "a multileader restyled");
    assert_ne!(
        scene.frame().segments(),
        &segments[..],
        "the note is drawn taller"
    );
    assert_eq!(
        cache.partial_build_count(),
        1,
        "the style change re-packed the chunk holding its multileader, not the scene (ADR-0022)"
    );
}

/// ADR-0021: a hatch is committed triangles — a style index each, batches
/// #206 (ADR-0031): a construction line is no committed geometry; the
/// overlay draws it clipped to the view, so panning far along it redraws
/// the overlay and leaves the scene as it was.
#[test]
fn a_construction_line_is_overlay_clipped_to_the_view_and_never_committed() {
    use cad_document::{Entity, XLineEntity};
    let line = XLineEntity::through(
        cad_core::Point2::new(0.0, 0.0),
        cad_core::Point2::new(1.0, 1.0),
    )
    .unwrap();
    let mut h = Harness::with_entities([Entity::from(line)]);
    let mut cache = SceneCache::new();
    let scene = cache.scene(h.session.drawing());
    assert_eq!(scene.frame().segment_count(), 0);
    let revision = scene.revision();
    let overlay = overlay_of(&h);
    let drawn = overlay.frame().segment_count();
    assert!(drawn >= 1, "{drawn}");
    // Far along it: still drawn, still nothing committed.
    h.enter_all(&["ZOOM", "C", "1000000,1000000", "10"]);
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
    assert!(overlay_of(&h).frame().segment_count() >= 1);
    // Off it: the overlay has no piece of it.
    h.enter_all(&["ZOOM", "C", "500,-500", "10"]);
    let off = overlay_of(&h).frame().segment_count();
    assert!(off < drawn, "{off} {drawn}");
}

/// #204: a wide polyline is committed as its band's triangles and no
/// strokes; its width set, the scene is rebuilt with them, and the
/// pointer never touches it.
#[test]
fn a_wide_polyline_is_committed_as_its_band_and_a_plain_one_as_strokes() {
    use cad_document::{Entity, PolylineEntity};
    let run = PolylineEntity::new(
        vec![
            cad_core::Point2::new(-50.0, 0.0),
            cad_core::Point2::new(50.0, 0.0),
            cad_core::Point2::new(50.0, 40.0),
        ],
        false,
    );
    let mut h = Harness::with_entities([Entity::from(run)]);
    let mut cache = SceneCache::new();
    let plain = cache.scene(h.session.drawing());
    assert_eq!(plain.frame().triangle_count(), 0);
    assert_eq!(plain.frame().segment_count(), 2);
    h.enter("PEDIT");
    h.click_world(cad_core::Point2::new(0.0, 0.0));
    h.enter_all(&["W", "6", ""]);
    let wide = cache.scene(h.session.drawing());
    assert_eq!(
        wide.frame().segment_count(),
        0,
        "the band, not the centre line"
    );
    assert!(
        wide.frame().triangle_count() >= 4,
        "{}",
        wide.frame().triangle_count()
    );
    let revision = wide.revision();
    h.move_to(300.0, 200.0);
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
}

/// that tile the buffer — beside the strokes of its boundary; the pointer
/// leaves them alone, and a fill being previewed travels in the overlay.
#[test]
fn a_hatch_is_committed_triangles_and_its_preview_overlay_triangles() {
    use cad_document::{Entity, PolylineEntity};
    let square = PolylineEntity::new(
        vec![
            cad_core::Point2::new(-50.0, -50.0),
            cad_core::Point2::new(50.0, -50.0),
            cad_core::Point2::new(50.0, 50.0),
            cad_core::Point2::new(-50.0, 50.0),
        ],
        true,
    );
    let mut h = Harness::with_entities([Entity::from(square)]);
    // The preview: triangles in the overlay, none committed.
    h.enter("HATCH");
    h.move_to(400.0, 300.0);
    let overlay = overlay_of(&h);
    assert_eq!(overlay.frame().triangle_count(), 2);
    let mut cache = SceneCache::new();
    assert_eq!(cache.scene(h.session.drawing()).frame().triangle_count(), 0);

    h.click(400.0, 300.0);
    let scene = cache.scene(h.session.drawing());
    let frame = scene.frame();
    assert_eq!(frame.triangle_count(), 2, "a square is two triangles");
    assert_eq!(
        frame.triangles().len(),
        2 * PackedFrame::FLOATS_PER_TRIANGLE
    );
    assert_eq!(scene.triangle_styles().len(), 2);
    let mut next = 0;
    for batch in frame.triangle_batches() {
        assert_eq!(batch.first, next);
        next += batch.count;
    }
    assert_eq!(next, 2, "the batches cover every triangle");
    assert!(frame.triangles().iter().all(|value| value.is_finite()));
    assert_frame_is_consistent(frame, "a hatch");
    let revision = scene.revision();
    h.move_to(100.0, 100.0);
    h.move_to(600.0, 200.0);
    assert_eq!(cache.scene(h.session.drawing()).revision(), revision);
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

/// #193: the scene of a layout holds the model's regions and the paper's;
/// a viewport moved re-packs the paper's chunk alone and leaves the
/// model's instances where they were; pointer movement on the paper
/// changes nothing committed.
#[test]
fn a_layouts_scene_is_the_model_through_its_viewports_and_the_paper_over_it() {
    let mut h = Harness::new();
    h.enter_all(&["REC", "0,0", "100,100"]);
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    h.enter_all(&["MVIEW", "20,20", "220,220", "MVIEW", "250,20", "350,120"]);
    h.enter_all(&["L", "20,250", "350,250", ""]);
    let space = h.session.space();
    let mut cache = SceneCache::new();
    let scene = cache.scene_in(h.session.drawing(), space);
    let regions = scene.regions();
    assert_eq!(regions.len(), 2, "{regions:?}");
    assert!(!regions[0].paper && regions[1].paper);
    assert_eq!(regions[0].count, [4, 0, 0], "the model's rectangle");
    assert_eq!(regions[1].count, [9, 0, 0], "two frames and a line");
    let model_bytes = scene.frame().segments()[..16].to_vec();
    cache.take_dirty();

    h.move_to(400.0, 300.0);
    cache.scene_in(h.session.drawing(), space);
    assert!(
        cache.take_dirty().is_empty(),
        "pointer movement changes nothing"
    );
    assert_eq!(cache.build_count(), 1);

    // The second viewport moved: the paper's chunk alone is re-packed.
    h.click_world(p(300.0, 120.0));
    h.enter_all(&["M", "0,0", "0,10"]);
    let scene = cache.scene_in(h.session.drawing(), space);
    let regions = scene.regions();
    let segments = scene.frame().segments().to_vec();
    assert_eq!(cache.partial_build_count(), 1);
    assert_eq!(regions[1].count, [9, 0, 0]);
    assert_eq!(
        &segments[..16],
        &model_bytes[..],
        "the model's instances, untouched"
    );
    let ranges = cache.take_dirty();
    assert_eq!(ranges.len(), 1, "{ranges:?}");
    assert_eq!(
        (ranges[0].first, ranges[0].count),
        (4, 9),
        "the paper's run"
    );
}

fn p(x: f64, y: f64) -> cad_core::Point2 {
    cad_core::Point2::new(x, y)
}
