//! The render-boundary suite (DESIGN §41, "Render boundary").
//!
//! One named test per rule in the design document, driven through the same
//! `Session` API the browser uses. If the boundary erodes, this is the file
//! that fails. The TypeScript half of the suite — the exported WASM API
//! snapshot and the calls-per-frame check — lives in
//! `web/src/cad/render-boundary.test.ts`.

use cad_commands::testing::Harness;
use cad_commands::{Dirty, Modifiers, PointerButton};
use cad_core::ScreenPoint;
use cad_render::{OverlayFrame, PackedFrame, SceneCache};

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
