//! MTEXT and the paragraph entity through the session (#144): creation,
//! the editor hand-off, editing, and transforms. Wrapping is tested in
//! `cad-text`; the box and mirroring in `cad-document/src/mtext.rs`.

use cad_core::Point2;
use cad_document::{Entity, LINE_SPACING, MTextEntity};

use crate::testing::Harness;
use crate::{OverlayItem, SnapMode};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn paragraphs(h: &Harness) -> Vec<MTextEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::MText(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn mtext_takes_two_corners_then_waits_for_the_editor() {
    let mut h = Harness::new();
    h.enter("MTEXT");
    assert_eq!(h.prompt(), "Specify first corner or [Height]:");
    assert_eq!(h.session.ui_state().text_editor, None);
    h.enter("10,50");
    assert_eq!(h.prompt(), "Specify opposite corner or [Height]:");
    // The box is previewed as the second corner is chosen.
    h.move_to_world(p(60.0, 20.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::MText(text)) if text.width == 50.0
    )));
    h.enter("60,20");
    assert_eq!(h.prompt(), "Enter text in the editor:");
    assert_eq!(
        h.session.ui_state().text_editor.as_deref(),
        Some(""),
        "the host opens its editor"
    );
    assert!(!h.session.ui_state().space_accepts);
    h.enter("First line of the note\nsecond");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.ui_state().text_editor, None);
    let note = &paragraphs(&h)[0];
    assert_eq!(
        note.insertion,
        p(10.0, 50.0),
        "the top left, whichever corner came first"
    );
    assert_eq!((note.width, note.height, note.rotation), (50.0, 2.5, 0.0));
    assert_eq!(note.content, "First line of the note\nsecond");
    assert_eq!(note.lines().len(), 2, "the width holds the first line");
    // The echo is one line.
    assert!(
        h.log()
            .iter()
            .any(|line| line.ends_with("First line of the note ⏎ second"))
    );

    // Corners the other way round; a height first; nothing written is nothing.
    h.enter_all(&["MT", "H", "5", "60,20", "10,50", "x"]);
    let second = &paragraphs(&h)[1];
    assert_eq!((second.insertion, second.height), (p(10.0, 50.0), 5.0));
    h.enter_all(&["MT", "0,0", "10,-10", "   "]);
    assert_eq!(paragraphs(&h).len(), 2);
    assert_eq!(h.prompt(), "Command:");
    h.undo();
    assert_eq!(paragraphs(&h).len(), 1);
}

#[test]
fn textedit_opens_the_editor_on_a_paragraph_and_replaces_its_words() {
    let mut h = Harness::new();
    h.enter_all(&["MTEXT", "0,0", "100,-30", "one two three"]);
    h.enter("ED");
    h.click_world(p(5.0, -2.0));
    assert_eq!(h.prompt(), "Edit the text in the editor:");
    assert_eq!(
        h.session.ui_state().text_editor.as_deref(),
        Some("one two three")
    );
    h.enter("one\ntwo\nthree");
    assert_eq!(paragraphs(&h)[0].content, "one\ntwo\nthree");
    assert_eq!(paragraphs(&h)[0].lines().len(), 3);
    h.undo();
    assert_eq!(paragraphs(&h)[0].content, "one two three");
    // Enter, or the same words, changes nothing.
    let revision = h.session.drawing().revision();
    h.enter("ED");
    h.click_world(p(5.0, -2.0));
    h.enter("");
    h.enter("ED");
    h.click_world(p(5.0, -2.0));
    h.enter("one two three");
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.session.ui_state().text_editor, None);
}

#[test]
fn a_paragraph_moves_scales_mirrors_and_is_summarised_like_a_text() {
    let mut h = Harness::new();
    h.enter_all(&["MTEXT", "0,0", "15,-20", "ab cd ef gh"]);
    let lines = paragraphs(&h)[0].lines().len();
    assert!(lines >= 2, "{lines}");
    h.click_world(p(5.0, -2.0));
    let summary = h.session.ui_state().selection;
    assert_eq!(summary.headline(), "1 multiline text");
    assert_eq!(summary.geometry[5], ("Lines".to_owned(), lines.to_string()));
    assert_eq!(h.session.grips().len(), 1);

    h.enter_all(&["M", "0,0", "100,0"]);
    assert_eq!(paragraphs(&h)[0].insertion, p(100.0, 0.0));
    h.escape();
    h.click_world(p(105.0, -2.0));
    h.enter_all(&["SC", "100,0", "2"]);
    assert_eq!(
        (paragraphs(&h)[0].height, paragraphs(&h)[0].width),
        (5.0, 30.0)
    );
    assert_eq!(
        paragraphs(&h)[0].lines().len(),
        lines,
        "wrapping scales with it"
    );
    h.escape();
    h.click_world(p(105.0, -2.0));
    h.enter_all(&["MI", "100,0", "100,10", "Y"]);
    let mirrored = &paragraphs(&h)[0];
    assert!(mirrored.rotation.abs() < 1e-9, "still reads left to right");
    assert!(
        (mirrored.insertion.x - 70.0).abs() < 1e-9,
        "{:?}",
        mirrored.insertion
    );
    h.escape();

    // The Insertion snap, and nothing on the glyphs.
    h.enter("L");
    h.move_to_world(p(71.0, 1.0));
    let snap = h.session.snap().expect("a snap");
    assert_eq!((snap.mode, snap.point), (SnapMode::Insertion, p(70.0, 0.0)));
    h.escape();

    // STRETCH moves it whole when its insertion point is held.
    h.enter_all(&["S", "65,5", "75,-5", "0,0", "0,-10"]);
    assert_eq!(paragraphs(&h)[0].insertion, p(70.0, -10.0));
    // The second line sits one spacing below the first.
    let strokes = paragraphs(&h)[0].strokes();
    let lowest = strokes
        .iter()
        .flatten()
        .map(|q| q.y)
        .fold(f64::INFINITY, f64::min);
    assert!(lowest < -10.0 - 5.0 * LINE_SPACING, "{lowest}");
}

#[test]
fn curve_commands_refuse_a_paragraph_by_name() {
    let mut h = Harness::new();
    h.enter_all(&["MTEXT", "0,0", "40,-20", "ab"]);
    h.enter_all(&["O", "3"]);
    h.click_world(p(5.0, -2.0));
    h.click_world(p(5.0, 30.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A multiline text cannot be offset."
    );
    h.escape();
    h.enter("BR");
    h.click_world(p(5.0, -2.0));
    h.click_world(p(15.0, -2.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A multiline text cannot be broken."
    );
}
