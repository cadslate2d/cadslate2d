//! TRIM (#97, #119) and EXTEND (#98, #119) through the session. The geometry
//! of every supported combination is tested in `cad-document/src/editing.rs`;
//! these tests cover the commands: choosing edges, picks, previews,
//! transactions, and messages.

use std::f64::consts::PI;

use cad_core::{EntityId, Point2};
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// A vertical line at `x`, from y = -60 to y = 60.
fn upright(x: f64) -> Entity {
    line(x, -60.0, x, 60.0)
}

/// A vertical edge at x = 40 and a horizontal line crossing it.
fn crossing() -> Harness {
    Harness::with_entities([upright(40.0), line(0.0, 0.0, 100.0, 0.0)])
}

/// Starts `command` with the objects at `edges` as its edges.
fn with_edges(h: &mut Harness, command: &str, edges: &[Point2]) {
    h.enter(command);
    for edge in edges {
        h.click_world(*edge);
    }
    h.enter("");
}

fn hovered(h: &Harness) -> Option<EntityId> {
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

// ── TRIM ──

#[test]
fn trim_removes_the_picked_side_of_the_crossing() {
    let mut h = crossing();
    let ids = h.ids();
    h.enter("TR");
    assert_eq!(h.prompt(), "Select cutting edges <all>:");
    h.click_world(p(40.0, 30.0));
    assert_eq!(h.log().last().unwrap(), "1 found, 1 total");
    h.enter("");
    assert_eq!(h.prompt(), "Select object to trim:");
    h.click_world(p(80.0, 0.0));
    assert_eq!(h.prompt(), "Select object to trim:", "it repeats");
    h.enter("");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.ids(), ids, "the trimmed line keeps its ID");
    assert_eq!(h.entities()[1], line(0.0, 0.0, 40.0, 0.0));
    assert_eq!(h.entities()[0], upright(40.0), "the edge is untouched");
    assert!(
        h.session.selection().is_empty(),
        "the edges do not stay selected"
    );
}

#[test]
fn a_pick_between_two_crossings_leaves_two_objects() {
    let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
    let mut h = Harness::with_entities([circle, line(-100.0, 0.0, 100.0, 0.0)]);
    with_edges(&mut h, "TRIM", &[p(0.0, 50.0)]);
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
    with_edges(&mut h, "TR", &[p(0.0, 70.0)]);
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
    let mut h = Harness::with_entities([upright(40.0), polyline, apart, circle]);
    let before = h.entities();
    let revision = h.session.drawing().revision();
    with_edges(&mut h, "TR", &[p(40.0, 50.0)]);

    h.click_world(p(80.0, 20.0));
    assert_eq!(
        h.log().last().unwrap(),
        "TRIM does not support polylines yet."
    );
    h.click_world(p(80.0, -40.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not cross a cutting edge."
    );
    h.click_world(p(-80.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not cross a cutting edge."
    );
    // The only edge, picked as a target: nothing else crosses it.
    h.click_world(p(40.0, -50.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object does not cross a cutting edge."
    );
    h.click_world(p(300.0, 200.0));
    assert_eq!(h.log().last().unwrap(), "No object there.");

    assert_eq!(h.prompt(), "Select object to trim:");
    assert_eq!(h.entities(), before);
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_circle_touched_once_says_it_needs_two_crossings() {
    let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
    let mut h = Harness::with_entities([line(-100.0, 50.0, 100.0, 50.0), circle]);
    with_edges(&mut h, "TR", &[p(-80.0, 50.0)]);
    h.click_world(p(0.0, -50.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A circle must be crossed twice by cutting edges to be trimmed."
    );
}

#[test]
fn every_trim_is_a_transaction_and_the_command_is_one_undo_step() {
    let mut h = Harness::with_entities([
        upright(40.0),
        line(0.0, 0.0, 100.0, 0.0),
        line(0.0, 30.0, 100.0, 30.0),
    ]);
    let before = h.entities();
    with_edges(&mut h, "TR", &[p(40.0, 50.0)]);
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
fn the_chosen_edges_stay_highlighted_while_objects_are_picked() {
    let mut h = crossing();
    let edge = h.ids()[0];
    with_edges(&mut h, "TR", &[p(40.0, 30.0)]);
    h.move_to_world(p(80.0, 5.0));
    assert!(h.session.overlay().contains(&OverlayItem::Highlight(edge)));
    h.escape();
    assert!(!h.session.overlay().contains(&OverlayItem::Highlight(edge)));
}

// ── several edges (#119) ──

/// Two edges: the pick is cut between the nearest crossings on either side.
#[test]
fn with_two_edges_the_part_between_them_can_be_removed() {
    let mut h = Harness::with_entities([upright(30.0), upright(70.0), line(0.0, 0.0, 100.0, 0.0)]);
    with_edges(&mut h, "TR", &[p(30.0, 40.0), p(70.0, 40.0)]);
    assert_eq!(
        h.log()
            .iter()
            .filter(|line| line.ends_with("total"))
            .count(),
        2
    );

    h.click_world(p(50.0, 0.0));
    h.enter("");
    let entities = h.entities();
    assert_eq!(entities.len(), 4);
    assert_eq!(entities[2], line(0.0, 0.0, 30.0, 0.0));
    assert_eq!(entities[3], line(70.0, 0.0, 100.0, 0.0));
}

#[test]
fn with_many_edges_each_pick_goes_to_the_edges_around_it() {
    let uprights = (1..=9).map(|step| upright(f64::from(step) * 10.0));
    let mut h = Harness::with_entities(uprights.chain([line(0.0, 0.0, 100.0, 0.0)]));
    h.enter("TR");
    h.select_box(p(105.0, 55.0), p(5.0, 45.0)); // a crossing box over all nine uprights
    assert_eq!(h.session.selection().len(), 9);
    h.enter("");

    h.click_world(p(45.0, 0.0)); // between the fourth and fifth
    h.click_world(p(3.0, 0.0)); // before the first
    h.enter("");
    let mut pieces: Vec<LineEntity> = h
        .lines()
        .into_iter()
        .filter(|line| line.start.y == 0.0 && line.end.y == 0.0)
        .collect();
    pieces.sort_by(|a, b| a.start.x.total_cmp(&b.start.x));
    assert_eq!(
        pieces,
        vec![
            LineEntity::new(p(10.0, 0.0), p(40.0, 0.0)),
            LineEntity::new(p(50.0, 0.0), p(100.0, 0.0))
        ]
    );
}

/// Enter with nothing selected makes every object an edge.
#[test]
fn enter_at_the_edges_prompt_makes_every_object_an_edge() {
    let mut h = Harness::with_entities([upright(30.0), upright(70.0), line(0.0, 0.0, 100.0, 0.0)]);
    h.enter_all(&["TR", ""]);
    assert_eq!(h.log().last().unwrap(), "Every object is an edge.");
    assert_eq!(h.prompt(), "Select object to trim:");

    h.click_world(p(85.0, 0.0));
    // The uprights are targets too: the top of the first one, above the line.
    h.click_world(p(30.0, 40.0));
    h.enter("");
    let entities = h.entities();
    assert_eq!(entities[2], line(0.0, 0.0, 70.0, 0.0));
    assert_eq!(entities[0], line(30.0, -60.0, 30.0, 0.0));
    assert_eq!(entities[1], upright(70.0));
}

/// Edges that are also targets work in both roles in one command.
#[test]
fn chosen_edges_can_be_trimmed_by_each_other() {
    // A plus sign: each line is the other's cutting edge.
    let mut h = Harness::with_entities([line(-50.0, 0.0, 50.0, 0.0), line(0.0, -50.0, 0.0, 50.0)]);
    with_edges(&mut h, "TR", &[p(-30.0, 0.0), p(0.0, 30.0)]);
    h.click_world(p(30.0, 0.0)); // the right arm of the horizontal
    h.click_world(p(0.0, -30.0)); // the lower arm of the vertical
    h.enter("");
    assert_eq!(
        h.entities(),
        vec![line(-50.0, 0.0, 0.0, 0.0), line(0.0, 0.0, 0.0, 50.0)]
    );
}

#[test]
fn a_selection_made_beforehand_is_not_taken_as_the_edges() {
    let mut h = Harness::with_entities([upright(30.0), upright(70.0), line(0.0, 0.0, 100.0, 0.0)]);
    h.click_world(p(30.0, 40.0));
    assert_eq!(h.session.selection().len(), 1);
    h.enter("TR");
    assert!(
        h.session.selection().is_empty(),
        "the edges are chosen afresh"
    );
    h.click_world(p(70.0, 40.0));
    h.enter("");
    // Only the upright at 70 is an edge: a pick left of it runs to the start.
    h.click_world(p(50.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(70.0, 0.0, 100.0, 0.0));
}

// ── hover feedback (#118) ──

#[test]
fn the_object_under_the_cursor_is_marked_while_a_command_asks_for_one() {
    let mut h = crossing();
    let ids = h.ids();
    // Not at the idle prompt, not at a point prompt, not while choosing edges.
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), None);
    h.enter_all(&["L", "200,200"]);
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), None);
    h.escape();
    h.enter("TR");
    h.move_to_world(p(40.0, 30.0));
    assert_eq!(hovered(&h), None);

    h.enter("");
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
    with_edges(&mut h, "TR", &[p(40.0, 30.0)]);
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
        upright(40.0),
        line(0.0, 0.0, 100.0, 0.0),
        line(60.0, -40.0, 100.0, -40.0),
    ]);
    let edge = h.ids()[0];
    with_edges(&mut h, "TR", &[p(40.0, 30.0)]);
    h.move_to_world(p(80.0, -40.0)); // does not cross the edge
    assert!(hovered(&h).is_some() && removal(&h).is_none());
    h.move_to_world(p(40.0, -30.0)); // the edge itself: nothing crosses it
    assert_eq!(hovered(&h), Some(edge));
    assert_eq!(removal(&h), None);
}

#[test]
fn extend_previews_the_object_as_a_click_would_leave_it() {
    let mut h = short_of_the_boundary();
    with_edges(&mut h, "EX", &[p(100.0, 40.0)]);
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
    assert_eq!(h.prompt(), "Select boundary edges <all>:");
    h.click_world(p(100.0, 40.0));
    h.enter("");
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
    with_edges(&mut h, "EXTEND", &[p(-60.0, 0.0)]);
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
    with_edges(&mut h, "EX", &[p(100.0, 50.0)]);

    let no_meeting = "The object does not meet a boundary edge when extended.";
    h.click_world(p(55.0, 0.0)); // passes below the boundary
    assert_eq!(h.log().last().unwrap(), no_meeting);
    h.click_world(p(5.0, 0.0)); // the far end points away from it
    assert_eq!(h.log().last().unwrap(), no_meeting);
    h.click_world(p(-80.0, 100.0));
    assert_eq!(
        h.log().last().unwrap(),
        "EXTEND does not support circles yet."
    );
    h.click_world(p(100.0, 30.0)); // the boundary itself has nothing to reach
    assert_eq!(h.log().last().unwrap(), no_meeting);

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
    with_edges(&mut h, "EX", &[p(-100.0, 40.0)]);
    h.click_world(p(5.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(-100.0, 0.0, 60.0, 0.0));
}

/// #119: several boundaries, and the all-edges default.
#[test]
fn extend_stops_at_the_first_of_several_boundaries() {
    let near = line(80.0, -80.0, 80.0, 80.0);
    let far = line(140.0, -80.0, 140.0, 80.0);
    let mut h = Harness::with_entities([far.clone(), near.clone(), line(0.0, 0.0, 60.0, 0.0)]);
    with_edges(&mut h, "EX", &[p(140.0, 40.0), p(80.0, 40.0)]);
    h.click_world(p(55.0, 0.0));
    assert_eq!(h.entities()[2], line(0.0, 0.0, 80.0, 0.0));
    // Picked again, its end is on the near boundary: on to the next one.
    h.click_world(p(75.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(0.0, 0.0, 140.0, 0.0));

    // With every object as an edge the same happens without choosing any.
    let mut h = Harness::with_entities([far, near, line(0.0, 0.0, 60.0, 0.0)]);
    h.enter_all(&["EX", ""]);
    assert_eq!(h.log().last().unwrap(), "Every object is an edge.");
    h.click_world(p(55.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(0.0, 0.0, 80.0, 0.0));
}
