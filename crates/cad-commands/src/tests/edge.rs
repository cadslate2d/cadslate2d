//! TRIM (#97) and EXTEND (#98) through the session. The geometry of every
//! supported combination is tested in `cad-document/src/editing.rs`; these
//! tests cover the commands: prompts, picks, transactions, and messages.

use std::f64::consts::PI;

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// A vertical edge at x = 40 and a horizontal line crossing it.
fn crossing() -> Harness {
    Harness::with_entities([line(40.0, -60.0, 40.0, 60.0), line(0.0, 0.0, 100.0, 0.0)])
}

// ── TRIM ──

#[test]
fn trim_removes_the_picked_side_of_the_crossing() {
    let mut h = crossing();
    let ids = h.ids();
    h.enter("TR");
    assert_eq!(h.prompt(), "Select cutting edge:");
    h.click_world(p(40.0, 30.0));
    assert_eq!(h.prompt(), "Select object to trim:");
    h.click_world(p(80.0, 0.0));
    assert_eq!(h.prompt(), "Select object to trim:", "it repeats");
    h.enter("");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.ids(), ids, "the trimmed line keeps its ID");
    assert_eq!(h.entities()[1], line(0.0, 0.0, 40.0, 0.0));
    assert_eq!(
        h.entities()[0],
        line(40.0, -60.0, 40.0, 60.0),
        "the edge is untouched"
    );
}

#[test]
fn a_pick_between_two_crossings_leaves_two_objects() {
    let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
    let mut h = Harness::with_entities([circle, line(-100.0, 0.0, 100.0, 0.0)]);
    h.enter("TRIM");
    h.click_world(p(0.0, 50.0));
    h.click_world(p(10.0, 0.0));
    h.enter("");

    let entities = h.entities();
    assert_eq!(entities.len(), 3);
    assert_eq!(entities[1], line(-100.0, 0.0, -50.0, 0.0));
    assert_eq!(entities[2], line(50.0, 0.0, 100.0, 0.0));
}

#[test]
fn trimming_a_circle_turns_it_into_an_arc_with_the_same_id() {
    let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
    let mut h = Harness::with_entities([line(0.0, -80.0, 0.0, 80.0), circle]);
    let id = h.ids()[1];
    h.enter("TR");
    h.click_world(p(0.0, 70.0));
    h.click_world(p(50.0, 0.0)); // the right half goes
    h.enter("");

    assert_eq!(h.ids()[1], id);
    let Entity::Arc(arc) = &h.entities()[1] else {
        panic!("a trimmed circle is an arc");
    };
    assert!(arc.arc().mid_point().distance_to(p(-50.0, 0.0)) < 1e-9);
    assert!((arc.arc().sweep - PI).abs() < 1e-9);
}

#[test]
fn trim_explains_what_it_cannot_do_and_changes_nothing() {
    let polyline: Entity = PolylineEntity::new(vec![p(0.0, 20.0), p(100.0, 20.0)], false).into();
    let apart = line(60.0, -40.0, 100.0, -40.0);
    let circle: Entity = CircleEntity::new(p(-100.0, 0.0), 20.0).into();
    let mut h = Harness::with_entities([line(40.0, -60.0, 40.0, 60.0), polyline, apart, circle]);
    let before = h.entities();
    let revision = h.session.drawing().revision();
    h.enter("TR");
    h.click_world(p(40.0, 50.0));

    h.click_world(p(80.0, 20.0));
    assert_eq!(
        h.log().last().unwrap(),
        "TRIM does not support polylines yet."
    );
    h.click_world(p(80.0, -40.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not cross the cutting edge."
    );
    h.click_world(p(-80.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not cross the cutting edge."
    );
    h.click_world(p(40.0, -50.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The cutting edge cannot trim itself."
    );
    h.click_world(p(300.0, 200.0));
    assert_eq!(h.log().last().unwrap(), "No object there.");

    assert_eq!(h.prompt(), "Select object to trim:");
    assert_eq!(h.entities(), before);
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_circle_touched_once_by_the_edge_says_it_needs_two_crossings() {
    let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
    let mut h = Harness::with_entities([line(-100.0, 50.0, 100.0, 50.0), circle]);
    h.enter("TR");
    h.click_world(p(-80.0, 50.0));
    h.click_world(p(0.0, -50.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A circle must cross the cutting edge twice to be trimmed."
    );
}

#[test]
fn every_trim_is_a_transaction_and_the_command_is_one_undo_step() {
    let mut h = Harness::with_entities([
        line(40.0, -60.0, 40.0, 60.0),
        line(0.0, 0.0, 100.0, 0.0),
        line(0.0, 30.0, 100.0, 30.0),
    ]);
    let before = h.entities();
    h.enter("TR");
    h.click_world(p(40.0, 50.0));
    let start = h.session.drawing().revision();
    h.click_world(p(80.0, 0.0));
    let after_one = h.session.drawing().revision();
    assert_ne!(after_one, start);
    h.click_world(p(10.0, 30.0));
    assert_ne!(h.session.drawing().revision(), after_one);
    h.enter("");

    assert_eq!(h.entities()[1], line(0.0, 0.0, 40.0, 0.0));
    assert_eq!(h.entities()[2], line(40.0, 30.0, 100.0, 30.0));
    h.undo();
    assert_eq!(h.entities(), before);
}

#[test]
fn the_chosen_edge_stays_highlighted_while_objects_are_picked() {
    let mut h = crossing();
    let edge = h.ids()[0];
    h.enter("TR");
    h.click_world(p(40.0, 30.0));
    h.move_to_world(p(80.0, 5.0));
    assert!(h.session.overlay().contains(&OverlayItem::Highlight(edge)));
    h.escape();
    assert!(!h.session.overlay().contains(&OverlayItem::Highlight(edge)));
}

// ── hover feedback (#118) ──

fn hovered(h: &Harness) -> Option<cad_core::EntityId> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::Hover(id) => Some(id),
        _ => None,
    })
}

fn removal(h: &Harness) -> Option<Entity> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewRemoval(entity) => Some(entity),
        _ => None,
    })
}

#[test]
fn the_object_under_the_cursor_is_marked_while_a_command_asks_for_one() {
    let mut h = crossing();
    let ids = h.ids();
    // Not at the idle prompt, and not at a point prompt.
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), None);
    h.enter_all(&["L", "200,200"]);
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), None);
    h.escape();

    h.enter("TR");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), Some(ids[0]));
    assert_eq!(h.session.hovered_object(), Some(ids[0]));
    h.move_to_world(p(80.0, 1.0));
    assert_eq!(hovered(&h), Some(ids[1]));
    h.move_to_world(p(150.0, 150.0));
    assert_eq!(hovered(&h), None);
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "hovering changes nothing"
    );
}

/// The previewed result equals what the click commits.
#[test]
fn trim_previews_the_part_a_click_would_remove() {
    let mut h = crossing();
    h.enter("TR");
    h.click_world(p(40.0, 30.0));
    let revision = h.session.drawing().revision();

    h.move_to_world(p(80.0, 1.0));
    assert_eq!(removal(&h), Some(line(40.0, 0.0, 100.0, 0.0)));
    h.move_to_world(p(10.0, 1.0));
    assert_eq!(removal(&h), Some(line(0.0, 0.0, 40.0, 0.0)));
    assert_eq!(h.session.drawing().revision(), revision);

    // What was previewed as removed is exactly what the click removes.
    h.click_world(p(10.0, 1.0));
    assert_eq!(h.entities()[1], line(40.0, 0.0, 100.0, 0.0));
}

#[test]
fn trim_previews_nothing_where_a_click_would_do_nothing() {
    let mut h = Harness::with_entities([
        line(40.0, -60.0, 40.0, 60.0),
        line(0.0, 0.0, 100.0, 0.0),
        line(60.0, -40.0, 100.0, -40.0),
    ]);
    let edge = h.ids()[0];
    h.enter("TR");
    // Before an edge is chosen there is nothing to preview, only the hover.
    h.move_to_world(p(80.0, 1.0));
    assert!(hovered(&h).is_some() && removal(&h).is_none());

    h.click_world(p(40.0, 30.0));
    h.move_to_world(p(80.0, -40.0)); // does not cross the edge
    assert!(hovered(&h).is_some() && removal(&h).is_none());
    h.move_to_world(p(40.0, -30.0)); // the edge itself
    assert_eq!(hovered(&h), Some(edge));
    assert_eq!(removal(&h), None);
}

#[test]
fn extend_previews_the_object_as_a_click_would_leave_it() {
    let mut h = short_of_the_boundary();
    h.enter("EX");
    h.click_world(p(100.0, 40.0));
    let revision = h.session.drawing().revision();

    h.move_to_world(p(55.0, 1.0));
    let previewed = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(entity) => Some(entity),
        _ => None,
    });
    assert_eq!(previewed, Some(line(0.0, 0.0, 100.0, 0.0)));
    assert_eq!(h.session.drawing().revision(), revision);

    h.click_world(p(55.0, 1.0));
    assert_eq!(Some(h.entities()[1].clone()), previewed);
}

#[test]
fn other_commands_that_pick_objects_mark_what_they_would_pick() {
    let mut h = crossing();
    let ids = h.ids();
    h.enter_all(&["O", "10"]);
    h.move_to_world(p(80.0, 1.0));
    assert_eq!(hovered(&h), Some(ids[1]));
    h.escape();

    h.enter("F");
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), Some(ids[0]));
}

// ── EXTEND ──

/// A vertical boundary at x = 100 and a horizontal line that stops short of it.
fn short_of_the_boundary() -> Harness {
    Harness::with_entities([line(100.0, -80.0, 100.0, 80.0), line(0.0, 0.0, 60.0, 0.0)])
}

#[test]
fn extend_carries_the_picked_end_on_to_the_boundary() {
    let mut h = short_of_the_boundary();
    let ids = h.ids();
    h.enter("EX");
    assert_eq!(h.prompt(), "Select boundary edge:");
    h.click_world(p(100.0, 40.0));
    assert_eq!(h.prompt(), "Select object to extend:");
    h.click_world(p(55.0, 0.0));
    assert_eq!(h.prompt(), "Select object to extend:", "it repeats");
    h.enter("");

    assert_eq!(h.ids(), ids);
    assert_eq!(h.entities()[1], line(0.0, 0.0, 100.0, 0.0));
    h.undo();
    assert_eq!(h.entities()[1], line(0.0, 0.0, 60.0, 0.0));
}

#[test]
fn extend_grows_an_arc_along_its_circle() {
    // A quarter arc about the origin from (50,0) to (0,50); the boundary is
    // the negative X axis.
    let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI / 2.0).into();
    let mut h = Harness::with_entities([line(-100.0, 0.0, -10.0, 0.0), arc]);
    h.enter("EXTEND");
    h.click_world(p(-60.0, 0.0));
    h.click_world(p(2.0, 50.0)); // near the arc's end
    h.enter("");

    let Entity::Arc(arc) = &h.entities()[1] else {
        panic!("an extended arc is an arc");
    };
    let arc = arc.arc();
    assert!(arc.start_point().distance_to(p(50.0, 0.0)) < 1e-9);
    assert!(arc.end_point().distance_to(p(-50.0, 0.0)) < 1e-9);
}

/// When the extension would not meet the boundary, a message is shown and
/// nothing changes.
#[test]
fn extend_explains_what_it_cannot_do_and_changes_nothing() {
    let circle: Entity = CircleEntity::new(p(-100.0, 100.0), 20.0).into();
    let mut h = Harness::with_entities([
        line(100.0, 20.0, 100.0, 80.0),
        line(0.0, 0.0, 60.0, 0.0),
        circle,
    ]);
    let before = h.entities();
    let revision = h.session.drawing().revision();
    h.enter("EX");
    h.click_world(p(100.0, 50.0));

    h.click_world(p(55.0, 0.0)); // passes below the boundary
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not meet the boundary edge when extended."
    );
    h.click_world(p(5.0, 0.0)); // the far end points away from it
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not meet the boundary edge when extended."
    );
    h.click_world(p(-80.0, 100.0));
    assert_eq!(
        h.log().last().unwrap(),
        "EXTEND does not support circles yet."
    );
    h.click_world(p(100.0, 30.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The boundary edge cannot be extended to itself."
    );

    assert_eq!(h.entities(), before);
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn the_end_nearer_to_the_pick_is_the_one_extended() {
    let mut h = Harness::with_entities([
        line(100.0, -80.0, 100.0, 80.0),
        line(-100.0, -80.0, -100.0, 80.0),
        line(0.0, 0.0, 60.0, 0.0),
    ]);
    // Towards the left boundary, from the start of the line.
    h.enter("EX");
    h.click_world(p(-100.0, 40.0));
    h.click_world(p(5.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(-100.0, 0.0, 60.0, 0.0));
}
