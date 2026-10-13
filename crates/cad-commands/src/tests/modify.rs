//! ERASE, MOVE, and COPY, in both verb–noun and noun–verb order.

use cad_core::{EntityId, Point2};
use cad_document::{CircleEntity, Entity, LineEntity};
use cad_geometry::Transform;

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A line from (-100,0) to (100,0) and a circle of radius 30 at (0,80).
fn scene() -> (Harness, EntityId, EntityId) {
    let mut h = Harness::new();
    h.enter_all(&["L", "-100,0", "100,0", ""]);
    h.enter_all(&["C", "0,80", "30"]);
    let ids = h.ids();
    (h, ids[0], ids[1])
}

fn selected(h: &Harness) -> Vec<EntityId> {
    h.session.selection().iter().collect()
}

fn line_of(h: &Harness, id: EntityId) -> LineEntity {
    *h.session
        .drawing()
        .entity(id)
        .and_then(Entity::as_line)
        .expect("a line")
}

// ── selecting for a command ──

#[test]
fn a_command_asks_for_objects_when_nothing_is_selected() {
    let (mut h, line, circle) = scene();
    h.enter("ERASE");
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );

    h.click_world(p(0.0, 0.0));
    assert_eq!(h.log().last().unwrap(), "1 found, 1 total");
    h.click_world(p(30.0, 80.0));
    assert_eq!(h.log().last().unwrap(), "1 found, 2 total");
    h.click_world(p(30.0, 80.0));
    assert_eq!(
        h.log().last().unwrap(),
        "0 found, 2 total",
        "already selected"
    );
    h.shift_click_world(p(0.0, 0.0));
    assert_eq!(h.log().last().unwrap(), "1 removed, 1 total");
    assert_eq!(selected(&h), vec![circle]);
    assert!(h.session.drawing().entity(line).is_some());
}

#[test]
fn typing_at_select_objects_is_explained() {
    let (mut h, ..) = scene();
    h.enter_all(&["E", "0,0"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Select objects by clicking them or with a window, then press Enter."
    );
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
}

#[test]
fn a_box_works_while_a_command_selects() {
    let (mut h, ..) = scene();
    h.enter("E");
    h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
    assert_eq!(h.log().last().unwrap(), "2 found, 2 total");
    h.enter("");
    assert!(h.entities().is_empty());
}

#[test]
fn escape_during_selection_leaves_the_document_unchanged() {
    let (mut h, ..) = scene();
    let revision = h.session.drawing().revision();
    h.enter("E");
    h.click_world(p(0.0, 0.0));
    h.escape();
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn finishing_with_nothing_selected_ends_the_command() {
    let (mut h, ..) = scene();
    for command in ["E", "M", "CO"] {
        h.enter_all(&[command, ""]);
        assert_eq!(h.log().last().unwrap(), "No objects selected.", "{command}");
        assert_eq!(h.prompt(), "Command:");
    }
    assert_eq!(h.entities().len(), 2);
}

// ── ERASE ──

#[test]
fn erase_removes_the_selection_in_one_transaction() {
    let (mut h, line, circle) = scene();
    h.enter("E");
    h.click_world(p(0.0, 0.0));
    h.click_world(p(30.0, 80.0));
    let dirty = h.enter("");

    assert!(dirty.contains(Dirty::SCENE));
    assert!(h.entities().is_empty());
    assert!(selected(&h).is_empty());
    assert_eq!(h.log().last().unwrap(), "2 erased.");

    // DESIGN §19.7: undo restores the same IDs and reselects them.
    h.undo();
    assert_eq!(h.ids(), vec![line, circle]);
    assert_eq!(selected(&h), vec![line, circle]);
}

#[test]
fn erase_with_a_preselection_acts_immediately() {
    let (mut h, line, circle) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter("ERASE");
    assert_eq!(
        h.prompt(),
        "Command:",
        "noun–verb: no Select objects prompt"
    );
    assert_eq!(h.ids(), vec![circle]);
    assert!(h.session.drawing().entity(line).is_none());
}

#[test]
fn delete_erases_the_selection_only_at_the_idle_prompt() {
    let (mut h, _, circle) = scene();
    assert!(h.delete().is_empty(), "nothing selected, nothing happens");

    h.click_world(p(0.0, 0.0));
    assert!(h.delete().contains(Dirty::SCENE));
    assert_eq!(h.ids(), vec![circle]);

    // During a command, Delete is not a shortcut for ERASE.
    h.enter_all(&["L", "0,0"]);
    assert!(h.delete().is_empty());
    assert_eq!(h.prompt(), "Specify next point or [Undo]:");
}

#[test]
fn the_index_follows_erase_undo_and_redo() {
    let (mut h, line, _) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter("E");
    h.click_world(p(0.0, 0.0));
    assert!(selected(&h).is_empty(), "the erased line cannot be picked");
    h.escape();

    h.undo();
    h.escape();
    h.click_world(p(0.0, 0.0));
    assert_eq!(selected(&h), vec![line], "restored and pickable again");

    h.redo();
    assert!(h.session.drawing().entity(line).is_none());
}

// ── MOVE ──

/// The headless test named in the issue: move by `@100,0`.
#[test]
fn move_shifts_every_selected_entity_by_exactly_the_given_vector() {
    let (mut h, line, circle) = scene();
    h.enter("MOVE");
    h.click_world(p(0.0, 0.0));
    h.click_world(p(30.0, 80.0));
    h.enter("");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify second point:");
    h.enter("@100,0");
    assert_eq!(h.prompt(), "Command:");

    assert_eq!(h.ids(), vec![line, circle], "IDs are unchanged");
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(0.0, 0.0), p(200.0, 0.0))
    );
    assert_eq!(
        h.session.drawing().entity(circle),
        Some(&Entity::Circle(CircleEntity::new(p(100.0, 80.0), 30.0)))
    );
    assert!(
        selected(&h).is_empty(),
        "the command ends with nothing selected"
    );
}

#[test]
fn move_is_one_undo_step() {
    let (mut h, line, circle) = scene();
    h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
    h.enter_all(&["M", "0,0", "0,50"]);

    h.undo();
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 0.0), p(100.0, 0.0))
    );
    assert_eq!(
        h.session.drawing().entity(circle),
        Some(&Entity::Circle(CircleEntity::new(p(0.0, 80.0), 30.0)))
    );
    h.redo();
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 50.0), p(100.0, 50.0))
    );
}

#[test]
fn move_accepts_clicks_and_direct_distance() {
    let (mut h, line, _) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter("M");
    h.click_world(p(0.0, 0.0)); // base point
    h.move_to(400.0, 100.0); // straight up from the base
    h.enter("25");
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 25.0), p(100.0, 25.0))
    );
}

#[test]
fn move_previews_transiently_and_names_its_sources() {
    let (mut h, line, _) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter_all(&["M", "0,0"]);
    let revision = h.session.drawing().revision();

    for step in 0..500 {
        let dirty = h.move_to(f64::from(step % 800), f64::from(step % 600));
        assert!(!dirty.contains(Dirty::SCENE));
    }
    h.move_to(450.0, 300.0); // 50 to the right
    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::PreviewTransformed {
        ids: vec![line],
        transform: Transform::translation(cad_core::Vec2::new(50.0, 0.0))
    }));
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "previewing never touches the document"
    );
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 0.0), p(100.0, 0.0))
    );
}

#[test]
fn cancelling_a_move_changes_nothing() {
    let (mut h, line, _) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter_all(&["M", "0,0"]);
    h.move_to(500.0, 100.0);
    h.escape();
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 0.0), p(100.0, 0.0))
    );
    assert!(
        !h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::PreviewTransformed { .. }))
    );
}

// ── COPY ──

/// The headless test named in the issue: two placements, two sets of copies.
#[test]
fn copy_places_repeatedly_and_leaves_the_sources() {
    let (mut h, line, circle) = scene();
    h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
    h.enter_all(&["COPY", "0,0", "0,200"]);
    assert_eq!(h.prompt(), "Specify second point:", "COPY keeps placing");
    h.enter_all(&["0,400", ""]);
    assert_eq!(h.prompt(), "Command:");

    assert_eq!(h.entities().len(), 6);
    assert_eq!(
        line_of(&h, line),
        LineEntity::new(p(-100.0, 0.0), p(100.0, 0.0)),
        "sources are untouched"
    );
    assert!(h.session.drawing().entity(circle).is_some());
    let lines = h.lines();
    assert_eq!(lines[1], LineEntity::new(p(-100.0, 200.0), p(100.0, 200.0)));
    assert_eq!(lines[2], LineEntity::new(p(-100.0, 400.0), p(100.0, 400.0)));
}

#[test]
fn copies_get_fresh_ids_and_one_undo_removes_the_whole_command() {
    let (mut h, line, circle) = scene();
    h.click_world(p(0.0, 0.0));
    h.enter_all(&["CO", "0,0", "0,10", "0,20", "0,30", ""]);
    let ids = h.ids();
    assert_eq!(ids.len(), 5);
    assert!(
        ids[2..].iter().all(|id| *id > circle),
        "copies are newer than every source"
    );

    h.undo();
    assert_eq!(h.ids(), vec![line, circle]);
}

#[test]
fn copy_aliases() {
    for alias in ["COPY", "CO", "CP"] {
        let (mut h, ..) = scene();
        h.click_world(p(0.0, 0.0));
        h.enter(alias);
        assert_eq!(h.session.ui_state().command, Some("COPY"), "{alias}");
    }
}
