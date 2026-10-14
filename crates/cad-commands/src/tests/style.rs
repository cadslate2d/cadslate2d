//! STYLE through the session (#221): every option and refusal, TEXT and
//! MTEXT taking the current style, and a style change leaving the texts
//! already made as they were.

use cad_document::{Entity, MTextEntity, TextEntity, TextStyle};

use crate::testing::Harness;

const OPTIONS: &str = "Press Enter to finish or [?/New/Set/Rename/Delete/Height/Width/Oblique]:";

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn current(h: &Harness) -> TextStyle {
    let drawing = h.session.drawing();
    drawing
        .text_styles()
        .get(drawing.current_text_style())
        .unwrap()
        .clone()
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

/// A session with a style `Notes` — 3.5 high, 0.7 wide, leaning 15° —
/// made current.
fn with_notes() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["STYLE", "N", "Notes", "H", "3.5", "W", "0.7", "O", "15", ""]);
    assert_eq!(h.prompt(), "Command:");
    h
}

#[test]
fn style_makes_sets_and_changes_the_current_text_style() {
    let mut h = Harness::new();
    h.enter("STYLE");
    assert!(said(
        &h,
        "Current text style \"Standard\": height asked, width 1, oblique 0°"
    ));
    assert_eq!(h.prompt(), OPTIONS);
    h.enter_all(&["N", "Notes"]);
    assert!(said(
        &h,
        "Text style \"Notes\" made from \"Standard\" and set current."
    ));
    h.enter_all(&["H", "3.5"]);
    assert!(said(&h, "\"Notes\": height 3.5, width 1, oblique 0°."));
    h.enter_all(&["W", "0.7", "O", "15"]);
    let notes = current(&h);
    assert_eq!(
        (notes.name.as_str(), notes.height, notes.width_factor),
        ("Notes", 3.5, 0.7)
    );
    assert!((notes.oblique.to_degrees() - 15.0).abs() < 1e-9);
    h.enter("?");
    assert!(said(
        &h,
        "  \"Notes\": height 3.5, width 0.7, oblique 15°, current"
    ));
    h.enter_all(&["S", "standard"]);
    assert!(said(&h, "Current text style: \"standard\"."));
    h.enter_all(&["R", "Notes", "Labels", ""]);
    assert!(h.session.drawing().text_styles().find("Labels").is_some());
    // The whole invocation is one undo.
    h.undo();
    assert!(h.session.drawing().text_styles().find("Notes").is_none());
}

#[test]
fn style_refuses_what_no_text_can_be_drawn_by_and_rows_in_use() {
    let mut h = with_notes();
    h.enter_all(&["STYLE", "W", "0"]);
    assert!(said(&h, "The width factor must be between 0.01 and 100."));
    h.enter_all(&["O", "89"]);
    assert!(said(&h, "The oblique angle must be between -85° and 85°."));
    h.enter_all(&["H", "-1"]);
    assert!(said(
        &h,
        "The height must be zero, to ask each time, or more."
    ));
    assert_eq!(current(&h).width_factor, 0.7, "nothing changed");
    h.enter_all(&["D", "Notes"]);
    assert!(said(
        &h,
        "Text style \"Notes\" is current: set another first."
    ));
    h.enter_all(&["D", "Standard"]);
    assert!(said(
        &h,
        "Text style \"Standard\" cannot be deleted or renamed."
    ));
    h.enter("");
    h.enter_all(&["TEXT", "0,0", "", "Kept"]);
    h.enter_all(&["STYLE", "S", "Standard", "D", "Notes", ""]);
    assert!(said(&h, "Text style \"Notes\" is used by a text."));
}

#[test]
fn text_takes_the_current_style_and_its_fixed_height() {
    let mut h = with_notes();
    h.enter_all(&["TEXT", "10,20"]);
    // The style fixes the height: it is not asked.
    assert_eq!(h.prompt(), "Specify rotation angle of text <0>:");
    h.enter_all(&["", "Room 12"]);
    let [text] = texts(&h).try_into().unwrap();
    let notes = h.session.drawing().text_styles().find("Notes").unwrap();
    assert_eq!(
        (text.style, text.height, text.width_factor),
        (notes, 3.5, 0.7)
    );
    assert!((text.oblique.to_degrees() - 15.0).abs() < 1e-9);
    let plain = TextEntity::new(text.insertion, 3.5, 0.0, "Room 12");
    assert!((text.width() - plain.width() * 0.7).abs() < 1e-9);

    // A style change leaves the text as it was; a new text takes it.
    h.enter_all(&["STYLE", "W", "2", ""]);
    assert_eq!(texts(&h)[0], text);
    h.enter_all(&["TEXT", "10,40", "", "Room 13"]);
    assert_eq!(texts(&h)[1].width_factor, 2.0);

    // A paragraph takes the style and its height.
    h.enter_all(&["MTEXT", "0,0", "50,-20"]);
    h.enter("Two words");
    let paragraph = h
        .entities()
        .into_iter()
        .find_map(|entity| match entity {
            Entity::MText(paragraph) => Some(paragraph),
            _ => None,
        })
        .unwrap();
    let expected: MTextEntity = paragraph.clone();
    assert_eq!((expected.style, expected.height), (notes, 3.5));
}
