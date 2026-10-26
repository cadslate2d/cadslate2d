//! TEXT, TEXTEDIT, and the text entity through the session (#143): the
//! commands, what the modify commands do with a text, the Insertion snap,
//! and the Properties summary. Geometry — box, picking, mirroring — is
//! tested in `cad-document/src/text.rs`.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_document::{Entity, Justification, LineEntity, TextEntity};

use crate::testing::Harness;
use crate::{OverlayItem, SnapMode};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn texts(h: &Harness) -> Vec<TextEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn text_asks_for_a_start_a_height_a_rotation_and_the_words() {
    let mut h = Harness::new();
    h.enter("TEXT");
    assert_eq!(h.prompt(), "Specify start point of text or [Justify]:");
    h.enter("10,20");
    assert_eq!(h.prompt(), "Specify height <2.5>:");
    h.enter("5");
    assert_eq!(h.prompt(), "Specify rotation angle of text <0>:");
    h.enter("90");
    assert_eq!(h.prompt(), "Enter text:");
    assert!(
        !h.session.ui_state().space_accepts,
        "spaces are part of a text"
    );
    h.enter("Ground floor");
    assert_eq!(h.prompt(), "Command:");
    let all = texts(&h);
    let [text] = all.as_slice() else {
        panic!("one text");
    };
    assert_eq!(text.content, "Ground floor");
    assert_eq!(text.insertion, p(10.0, 20.0));
    assert_eq!(text.height, 5.0);
    assert!((text.rotation - FRAC_PI_2).abs() < 1e-12);
    assert_eq!(text.justification, Justification::Left);
    assert_eq!(h.session.ui_state().selection.count, 0);

    // Defaults by Enter; the height can also be shown with the pointer.
    h.enter_all(&["DT", "0,0", "", "", "Abc"]);
    let second = &texts(&h)[1];
    assert_eq!((second.height, second.rotation), (2.5, 0.0));
    h.enter_all(&["TEXT", "0,-100"]);
    h.click_world(p(0.0, -96.0));
    assert_eq!(h.prompt(), "Specify rotation angle of text <0>:");
    h.enter_all(&["", "x"]);
    assert_eq!(texts(&h)[2].height, 4.0);

    // A text is one undo step; nothing typed is no text.
    h.undo();
    assert_eq!(texts(&h).len(), 2);
    h.enter_all(&["TEXT", "0,0", "", "", ""]);
    assert_eq!(texts(&h).len(), 2);
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["TEXT", "0,0", "0"]);
    assert_eq!(last(&h), "The height must be greater than zero.");
    assert_eq!(h.prompt(), "Specify height <2.5>:");
    h.escape();
}

#[test]
fn justification_is_chosen_first_and_the_insertion_point_follows_it() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "J"]);
    assert_eq!(h.prompt(), "Enter justification or [Left/Center/Right]:");
    h.enter_all(&["C", "50,0", "4", "0", "MID"]);
    let text = &texts(&h)[0];
    assert_eq!(text.justification, Justification::Center);
    assert_eq!(text.insertion, p(50.0, 0.0));
    let frame = text.frame();
    assert!(
        (frame.start.x + frame.width / 2.0 - 50.0).abs() < 1e-9,
        "centred on the insertion"
    );
    h.enter_all(&["TEXT", "J", "R", "50,10", "4", "0", "END"]);
    let right = &texts(&h)[1];
    assert!((right.frame().start.x + right.width() - 50.0).abs() < 1e-9);
}

#[test]
fn the_preview_shows_a_sample_at_the_height_and_angle_being_chosen() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "0,0"]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(0.0, 8.0));
    let sample = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Text(text)) => Some(text),
        _ => None,
    });
    assert_eq!(sample.map(|text| text.height), Some(8.0));
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "a preview commits nothing"
    );
    h.enter("8");
    h.move_to_world(p(10.0, 10.0));
    let sample = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Text(text)) => Some(text),
        _ => None,
    });
    assert!(sample.is_some_and(|text| (text.rotation - 45f64.to_radians()).abs() < 1e-9));
    h.escape();
}

#[test]
fn textedit_changes_what_a_picked_text_says_and_refuses_other_objects() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 50.0), p(100.0, 50.0)).into()]);
    h.enter_all(&["TEXT", "0,0", "10", "0", "Hello"]);
    let width = texts(&h)[0].width();
    h.enter("TEXTEDIT");
    assert_eq!(h.prompt(), "Select text to edit:");
    h.click_world(p(50.0, 50.0));
    assert_eq!(last(&h), "That is a line, not a text.");
    assert_eq!(h.prompt(), "Select text to edit:");
    // Inside the first letter's counter: picked by the box, not the strokes.
    h.click_world(p(width * 0.1, 5.0));
    assert_eq!(h.prompt(), "Enter new text <Hello>:");
    h.enter("Goodbye");
    assert_eq!(texts(&h)[0].content, "Goodbye");
    assert_eq!(texts(&h)[0].height, 10.0, "only the words change");
    assert_eq!(h.prompt(), "Command:");
    h.undo();
    assert_eq!(texts(&h)[0].content, "Hello");
    // Enter keeps the old text and changes nothing.
    let revision = h.session.drawing().revision();
    h.enter("ED");
    h.click_world(p(width * 0.5, 5.0));
    h.enter("");
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_text_moves_copies_turns_scales_and_mirrors_whole_and_stays_readable() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "0,0", "10", "0", "AB"]);
    let width = texts(&h)[0].width();
    let on_text = p(width / 2.0, 5.0);
    h.click_world(on_text);
    assert_eq!(
        h.session.selection().len(),
        1,
        "a click on the box selects it"
    );
    h.enter_all(&["M", "0,0", "100,0"]);
    assert_eq!(texts(&h)[0].insertion, p(100.0, 0.0));
    h.escape();

    h.click_world(p(100.0 + width / 2.0, 5.0));
    h.enter_all(&["SC", "100,0", "2"]);
    assert_eq!(texts(&h)[0].height, 20.0);
    h.escape();

    h.click_world(p(100.0 + width, 10.0));
    h.enter_all(&["RO", "100,0", "90"]);
    assert!((texts(&h)[0].rotation - FRAC_PI_2).abs() < 1e-9);
    h.escape();

    // Mirrored across the vertical axis through its insertion: the box
    // flips to the other side and the text still reads upward.
    h.click_world(p(90.0, width));
    h.enter_all(&["MI", "100,0", "100,10", "N"]);
    assert_eq!(texts(&h).len(), 2, "the source is kept");
    let mirrored = &texts(&h)[1];
    assert!((mirrored.rotation - FRAC_PI_2).abs() < 1e-9);
    // The cap box was left of x = 100 and is now right of it: the baseline
    // moved to its far edge, 20 (the height) to the right.
    assert!(
        (mirrored.start().x - 120.0).abs() < 1e-9,
        "{:?}",
        mirrored.start()
    );
    h.escape();

    // A copy is another text with the same words and its own place.
    h.click_world(p(110.0, width));
    h.enter_all(&["CO", "100,0", "200,0", ""]);
    assert_eq!(texts(&h).len(), 3);
    assert_eq!(texts(&h)[2].content, "AB");
    assert!((texts(&h)[2].start().x - 220.0).abs() < 1e-9);
}

#[test]
fn commands_that_cut_or_join_curves_refuse_a_text_by_name() {
    let mut h = Harness::with_entities([LineEntity::new(p(-50.0, 5.0), p(150.0, 5.0)).into()]);
    h.enter_all(&["TEXT", "0,0", "10", "0", "AB"]);
    let width = texts(&h)[0].width();
    let on_text = p(width / 2.0, 7.0);

    h.enter_all(&["O", "3"]);
    h.click_world(on_text);
    h.click_world(p(width / 2.0, 30.0));
    assert_eq!(last(&h), "A text cannot be offset.");
    h.escape();

    h.enter("BR");
    h.click_world(on_text);
    h.click_world(p(width * 0.8, 7.0));
    assert_eq!(last(&h), "A text cannot be broken.");
    h.escape();

    h.enter_all(&["TR", ""]);
    h.click_world(on_text);
    assert!(last(&h).contains("text"), "{}", last(&h));
    h.escape();

    h.enter_all(&["EX", ""]);
    h.click_world(on_text);
    assert!(last(&h).contains("text"), "{}", last(&h));
    h.escape();

    h.enter("F");
    h.click_world(p(-40.0, 5.0));
    h.click_world(on_text);
    assert!(last(&h).contains("text"), "{}", last(&h));
    h.escape();

    // A text is never a cutting edge: the line through it is not cut by it.
    h.enter("TR");
    h.click_world(on_text);
    h.enter("");
    h.click_world(p(-40.0, 5.0));
    assert_eq!(h.entities().len(), 2, "nothing trimmed");
    h.escape();

    // JOIN leaves it alone.
    h.enter("J");
    h.click_world(p(-40.0, 5.0));
    h.click_world(on_text);
    h.enter("");
    assert!(
        last(&h).contains("text") || last(&h).contains("Nothing"),
        "{}",
        last(&h)
    );
}

#[test]
fn stretch_moves_a_text_whose_insertion_point_is_in_the_window() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "0,0", "10", "0", "AB"]);
    h.enter_all(&["TEXT", "100,0", "10", "0", "CD"]);
    h.enter_all(&["S", "-5,-5", "5,20", "0,0", "0,30"]);
    assert_eq!(texts(&h)[0].insertion, p(0.0, 30.0));
    assert_eq!(texts(&h)[1].insertion, p(100.0, 0.0));
}

#[test]
fn the_insertion_snap_lands_on_the_insertion_point_and_nothing_else_on_a_text() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "40,30", "10", "0", "AB"]);
    assert!(
        h.session.snap_modes().is_on(SnapMode::Insertion),
        "on by default"
    );
    h.enter("L");
    h.move_to_world(p(41.0, 31.0));
    let snap = h.session.snap().expect("a snap");
    assert_eq!(
        (snap.mode, snap.point),
        (SnapMode::Insertion, p(40.0, 30.0))
    );
    // On a glyph stroke, away from the insertion: no Endpoint, Midpoint,
    // or Nearest offered by the text.
    h.enter_all(&["OSNAP", "NEA", ""]);
    h.enter("L");
    h.move_to_world(p(60.0, 36.0));
    assert!(h.session.snap().is_none(), "{:?}", h.session.snap());
    h.escape();
    h.enter_all(&["OSNAP", "INS", ""]);
    assert!(!h.session.snap_modes().is_on(SnapMode::Insertion));
    h.enter("L");
    h.move_to_world(p(41.0, 31.0));
    assert!(h.session.snap().is_none());
}

#[test]
fn a_text_has_an_insertion_grip_that_moves_it_and_is_summarised() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "0,0", "10", "30", "Hi"]);
    let width = texts(&h)[0].width();
    h.click_world(p(width * 0.4, 5.0));
    let grips = h.session.grips();
    assert_eq!(grips.len(), 1);
    assert_eq!(grips[0].position, p(0.0, 0.0));
    let summary = h.session.ui_state().selection;
    assert_eq!(summary.headline(), "1 text");
    assert_eq!(
        summary.geometry[0],
        ("Contents".to_owned(), "Hi".to_owned())
    );
    assert_eq!(
        summary.geometry[3],
        ("Rotation".to_owned(), "30.0000°".to_owned())
    );
    assert_eq!(
        summary.geometry[4],
        ("Justification".to_owned(), "Left".to_owned())
    );
    // Drag the grip: the text moves whole.
    h.click_world(p(0.0, 0.0));
    h.click_world(p(50.0, 50.0));
    assert_eq!(texts(&h)[0].insertion, p(50.0, 50.0));
    assert_eq!(texts(&h)[0].content, "Hi");
}

/// A double click on a text edits it (the maintainer, 2026-10-26): TEXTEDIT
/// with it picked, the editor open with its words on one line; on its
/// insertion grip too, the grip the clicks took let go; on anything else,
/// nothing.
#[test]
fn a_double_click_on_a_text_edits_it_in_the_editor() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 50.0), p(100.0, 50.0)).into()]);
    h.enter_all(&["TEXT", "0,0", "10", "0", "Hello"]);
    let width = texts(&h)[0].width();
    let on = |h: &Harness, point: Point2| h.session.view().world_to_screen(point);

    // The two clicks select it; the double click edits it.
    let at = on(&h, p(width * 0.5, 5.0));
    h.click(at.x, at.y);
    h.click(at.x, at.y);
    h.session.double_click(at);
    assert_eq!(h.prompt(), "Enter new text <Hello>:");
    let ui = h.session.ui_state();
    assert_eq!(ui.text_editor.as_deref(), Some("Hello"));
    assert!(ui.text_editor_line, "a text's words are one line");
    h.enter("Goodbye");
    assert_eq!(texts(&h)[0].content, "Goodbye");
    assert_eq!(h.prompt(), "Command:");
    h.undo();
    assert_eq!(texts(&h)[0].content, "Hello", "one undo");

    // On the insertion grip: the clicks took the grip, which is let go.
    h.session.key(crate::KeyAction::Escape);
    let grip = on(&h, p(0.0, 0.0));
    h.click(grip.x, grip.y);
    h.click(grip.x, grip.y);
    h.session.double_click(grip);
    assert_eq!(h.prompt(), "Enter new text <Hello>:");
    assert!(
        !h.log().iter().any(|line| line == "*Cancel*"),
        "{:?}",
        h.log()
    );
    h.session.key(crate::KeyAction::Escape);

    // On a line: nothing is started.
    h.session.key(crate::KeyAction::Escape);
    let line = on(&h, p(80.0, 50.0));
    h.click(line.x, line.y);
    h.click(line.x, line.y);
    h.session.double_click(line);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.ui_state().text_editor, None);
}
