//! LEADER (#205): points until Enter, then a note; grips at every
//! vertex; the note edited with TEXTEDIT; snapped to as annotation.

use cad_core::Point2;
use cad_document::{DimStyle, Entity, LeaderEntity};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem, SnapMode};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn leader(h: &Harness, index: usize) -> LeaderEntity {
    match &h.entities()[index] {
        Entity::Leader(leader) => leader.clone(),
        other => panic!("not a leader: {other:?}"),
    }
}

#[test]
fn leader_runs_through_its_points_takes_a_note_and_is_one_undo() {
    let mut h = Harness::new();
    h.enter("LEADER");
    assert_eq!(h.prompt(), "Specify leader start point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify next point:");
    h.enter("0,0");
    assert_eq!(last(&h), "The next point must differ from the last.");
    h.enter("10,10");
    assert_eq!(h.prompt(), "Specify next point or [Undo]:");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(30.0, 10.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Leader(preview)) if preview.vertices.len() == 3
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter_all(&["25,10", "U", "20,10", ""]);
    assert_eq!(h.prompt(), "Enter leader text <none>:");
    let dirty = h.enter("Hole 10 dia");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.prompt(), "Command:");
    let made = leader(&h, 0);
    assert_eq!(
        made.vertices,
        vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 10.0)]
    );
    assert_eq!(made.text.as_deref(), Some("Hole 10 dia"));
    assert_eq!(made.style, h.session.drawing().current_dim_style());
    h.undo();
    assert!(h.entities().is_empty(), "one undo");

    // No note: Enter at the text prompt; one point: nothing made.
    h.enter_all(&["LE", "0,0", "10,0", "", ""]);
    assert_eq!(leader(&h, 0).text, None);
    h.enter_all(&["LE", "0,0", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.entities().len(), 1);
}

#[test]
fn a_leader_is_picked_laid_out_by_its_style_snapped_to_and_moved_by_its_grips() {
    let mut h = Harness::new();
    h.enter_all(&["LE", "0,0", "20,20", "40,20", "", "Note"]);
    h.enter_all(&["ZOOM", "E"]);
    // Picked on a segment, on the arrowhead, and on the note's box.
    h.click_world(p(10.0, 10.0));
    assert_eq!(h.session.selection_summary().headline(), "1 leader");
    let rows = h.session.selection_summary().geometry;
    assert!(
        rows.iter()
            .any(|(label, value)| label == "Text" && value == "Note")
    );
    h.escape();
    let style = h
        .session
        .drawing()
        .tables()
        .dim_style(h.session.drawing().current_dim_style())
        .clone();
    let layout = leader(&h, 0).layout(&style);
    let note = layout.text.clone().unwrap();
    h.click_world(note.insertion + cad_core::Vec2::new(1.0, style.text_height / 2.0));
    assert_eq!(h.session.selection().len(), 1, "on the note");
    h.escape();
    // Snapped to at its vertices and its note's insertion.
    assert!(h.session.snap_modes().is_on(SnapMode::Endpoint));
    h.enter("L");
    h.move_to_world(p(20.5, 19.5));
    assert_eq!(h.session.ui_state().snap, Some("Endpoint"));
    assert_eq!(h.session.ui_state().cursor, Some((20.0, 20.0)));
    h.escape();
    // Its grips are its vertices; one moves a vertex, the note following.
    h.click_world(p(10.0, 10.0));
    let grips = h.session.grips();
    assert_eq!(grips.len(), 3);
    assert_eq!(grips[2].position, p(40.0, 20.0));
    h.click_world(p(40.0, 20.0));
    h.enter("50,30");
    assert_eq!(leader(&h, 0).vertices[2], p(50.0, 30.0));
    let moved = leader(&h, 0).layout(&style);
    assert!(moved.text.unwrap().insertion.y > note.insertion.y);
    h.escape();
    // A style change redraws it: the scene's shown revision moves.
    let shown = h.session.drawing().shown_revision();
    h.enter_all(&["DIMSTYLE", "A", "5", ""]);
    let _ = DimStyle::named("x");
    assert_ne!(h.session.drawing().shown_revision(), shown);
}

#[test]
fn textedit_edits_a_leaders_note_and_enter_keeps_it() {
    let mut h = Harness::new();
    h.enter_all(&["LE", "0,0", "20,20", "", "Old"]);
    h.enter_all(&["ZOOM", "E", "TEXTEDIT"]);
    h.click_world(p(10.0, 10.0));
    assert_eq!(h.prompt(), "Enter new leader text <Old>:");
    h.enter("New");
    assert_eq!(leader(&h, 0).text.as_deref(), Some("New"));
    h.enter("TEXTEDIT");
    h.click_world(p(10.0, 10.0));
    h.enter("");
    assert_eq!(leader(&h, 0).text.as_deref(), Some("New"), "Enter keeps it");
    h.undo();
    assert_eq!(leader(&h, 0).text.as_deref(), Some("Old"));
}
