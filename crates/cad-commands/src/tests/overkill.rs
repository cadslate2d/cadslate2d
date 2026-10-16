//! OVERKILL through the session (#234): duplicates of each kind removed,
//! a line inside another, overlapping and touching lines merged, objects
//! that differ in a property kept apart unless it is ignored, one undo.

use cad_core::Point2;
use cad_document::{Entity, LineEntity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

#[test]
fn duplicates_go_and_collinear_lines_merge_as_one_undo() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "50,0", ""]);
    h.enter_all(&["L", "40,0", "80,0", ""]);
    h.enter_all(&["L", "80,0", "100,0", ""]);
    h.enter_all(&["L", "10,0", "20,0", ""]);
    h.enter_all(&["C", "0,50", "10", "C", "0,50", "10"]);
    h.enter_all(&["A", "100,50", "120,70", "140,50"]);
    h.enter_all(&["A", "100,50", "120,70", "140,50"]);
    h.enter_all(&["PLINE", "0,100", "50,100", "50,120", ""]);
    h.enter_all(&["PLINE", "0,100", "50,100", "50,120", ""]);
    h.enter_all(&["POINT", "200,200", "200,200", ""]);
    h.enter_all(&["TEXT", "0,-20", "5", "0", "Same"]);
    h.enter_all(&["TEXT", "0,-20", "5", "0", "Same"]);
    let before = h.entities().len();
    h.select_box(p(-50.0, -50.0), p(300.0, 300.0));
    h.enter("OVERKILL");
    assert_eq!(h.prompt(), "Enter an option or [Ignore] <done>:");
    h.enter("");
    assert!(said(
        &h,
        "8 objects removed; 1 line lengthened to cover them."
    ));
    assert_eq!(h.entities().len(), before - 8);
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(0.0, 0.0), p(100.0, 0.0))],
        "the oldest, covering all four"
    );
    h.undo();
    assert_eq!(h.entities().len(), before, "one undo");

    // Run again on what is left: nothing to do.
    h.redo();
    h.select_box(p(-50.0, -50.0), p(300.0, 300.0));
    h.enter_all(&["OVERKILL", ""]);
    assert!(said(&h, "No duplicates found."));
}

#[test]
fn a_property_that_differs_keeps_objects_apart_unless_ignored() {
    let mut h = Harness::new();
    h.enter_all(&["C", "0,0", "10"]);
    h.enter_all(&["COLOR", "red", "C", "0,0", "10", "COLOR", "bylayer"]);
    h.select_box(p(-50.0, -50.0), p(50.0, 50.0));
    h.enter_all(&["OVERKILL", ""]);
    assert!(said(&h, "No duplicates found."));
    h.select_box(p(-50.0, -50.0), p(50.0, 50.0));
    h.enter_all(&["OVERKILL", "I", "C"]);
    assert!(said(&h, "Ignoring colour."));
    h.enter_all(&["", ""]);
    assert!(said(&h, "1 duplicate removed."));
    let circles = h
        .entities()
        .into_iter()
        .filter(|entity| matches!(entity, Entity::Circle(_)))
        .count();
    assert_eq!(circles, 1);
}
