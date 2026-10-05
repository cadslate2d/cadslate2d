//! BREAK (#126) through the session. What breaking does to each kind of
//! entity is tested in `cad-document/src/editing.rs`; these tests cover the
//! command: the pick as first point, the `First` option, `@`, refusals,
//! and transactions.

use std::f64::consts::{FRAC_PI_2, PI};

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

fn close(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-9
}

fn one_line() -> Harness {
    Harness::with_entities([line(0.0, 0.0, 100.0, 0.0)])
}

#[test]
fn the_pick_is_the_first_point_and_the_part_up_to_the_second_goes() {
    let mut h = one_line();
    let ids = h.ids();
    let revision = h.session.drawing().revision();
    h.enter("BR");
    assert_eq!(h.prompt(), "Select object:");
    h.click_world(p(30.0, 0.0));
    assert_eq!(h.prompt(), "Specify second break point or [First]:");
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::Highlight(ids[0])),
        "the object being broken is shown"
    );
    assert_eq!(h.session.drawing().revision(), revision, "nothing yet");
    h.click_world(p(70.0, 0.0));

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 30.0, 0.0), line(70.0, 0.0, 100.0, 0.0)]
    );
    assert_eq!(h.ids()[0], ids[0], "the first piece is the object, edited");

    h.undo();
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)], "one step");
    h.redo();
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn an_at_sign_breaks_at_one_point_and_removes_nothing() {
    let mut h = one_line();
    h.enter("BREAK");
    h.click_world(p(40.0, 0.0));
    h.enter("@");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 40.0, 0.0), line(40.0, 0.0, 100.0, 0.0)]
    );
}

#[test]
fn the_first_option_names_the_first_point_instead_of_the_pick() {
    let mut h = one_line();
    h.enter("BR");
    h.click_world(p(90.0, 0.0));
    h.enter("F");
    assert_eq!(h.prompt(), "Specify first break point:");
    h.enter("20,0");
    assert_eq!(h.prompt(), "Specify second break point or [First]:");
    h.enter("@40,0");
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 20.0, 0.0), line(60.0, 0.0, 100.0, 0.0)]
    );
}

#[test]
fn a_second_point_past_the_end_takes_the_end_off() {
    let mut h = one_line();
    let ids = h.ids();
    h.enter("BR");
    h.click_world(p(75.0, 0.0));
    h.enter("150,20");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 75.0, 0.0)]);
    assert_eq!(h.ids(), ids);
}

/// Breaking at an endpoint is refused with a message.
#[test]
fn breaking_at_an_end_is_refused_with_a_message_and_asks_again() {
    let mut h = one_line();
    let revision = h.session.drawing().revision();
    h.enter("BR");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["F", "100,0", "@"]);
    assert_eq!(
        h.log().last().unwrap(),
        "That is an end of the object: there is nothing to break off."
    );
    assert_eq!(h.prompt(), "Specify second break point or [First]:");
    assert_eq!(h.session.drawing().revision(), revision);

    // From one end to the other is the whole object.
    h.enter("0,0");
    assert_eq!(
        h.log().last().unwrap(),
        "That would remove the whole object; use ERASE for that."
    );
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);

    // The command is still usable.
    h.enter("60,0");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 60.0, 0.0)]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn an_arc_is_broken_into_two_arcs() {
    let mut h = Harness::with_entities([ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into()]);
    h.enter("BR");
    h.click_world(p(
        50.0 * 60f64.to_radians().cos(),
        50.0 * 60f64.to_radians().sin(),
    ));
    h.enter("0,80"); // lands on the top of the arc
    let entities = h.entities();
    let [Entity::Arc(first), Entity::Arc(second)] = entities.as_slice() else {
        panic!("two arcs, got {entities:?}");
    };
    assert!(close(first.arc().start_point(), p(50.0, 0.0)));
    assert!((first.arc().sweep - 60f64.to_radians()).abs() < 1e-9);
    assert!(close(second.arc().start_point(), p(0.0, 50.0)));
    assert!(close(second.arc().end_point(), p(-50.0, 0.0)));
}

#[test]
fn a_circle_becomes_an_arc_and_cannot_be_broken_at_one_point() {
    let mut h = Harness::with_entities([CircleEntity::new(p(0.0, 0.0), 50.0).into()]);
    let ids = h.ids();
    h.enter("BR");
    h.click_world(p(50.0, 0.0));
    h.enter("@");
    assert_eq!(
        h.log().last().unwrap(),
        "A circle has no ends: it needs two different break points."
    );
    assert_eq!(h.prompt(), "Specify second break point or [First]:");

    // Counter-clockwise from the first point to the second goes.
    h.enter("0,50");
    let entities = h.entities();
    let [Entity::Arc(arc)] = entities.as_slice() else {
        panic!("one arc, got {entities:?}");
    };
    assert!(close(arc.arc().start_point(), p(0.0, 50.0)));
    assert!(close(arc.arc().end_point(), p(50.0, 0.0)));
    assert!((arc.arc().sweep - 3.0 * FRAC_PI_2).abs() < 1e-9);
    assert_eq!(h.ids(), ids, "the arc takes the circle's place");
    h.undo();
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(0.0, 0.0), 50.0).into()]
    );
}

#[test]
fn an_open_polyline_is_broken_into_two_and_keeps_its_arcs() {
    // A straight run, then a half circle bulging down from (100, 0) to (200, 0).
    let source = PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(200.0, 0.0)],
        vec![0.0, 1.0],
        false,
    );
    let mut h = Harness::with_entities([source.into()]);
    h.enter("BR");
    h.click_world(p(60.0, 0.0));
    h.enter("150,-50"); // the bottom of the half circle
    let entities = h.entities();
    let [Entity::Polyline(first), Entity::Polyline(second)] = entities.as_slice() else {
        panic!("two polylines, got {entities:?}");
    };
    assert_eq!(first.vertices, vec![p(0.0, 0.0), p(60.0, 0.0)]);
    assert!(!first.has_arcs());
    assert!(close(second.vertices[0], p(150.0, -50.0)));
    assert!(close(second.vertices[1], p(200.0, 0.0)));
    assert!(
        (second.bulge(0) - (PI / 8.0).tan()).abs() < 1e-9,
        "a quarter circle is left of the half"
    );
}

#[test]
fn a_closed_polyline_opens_and_needs_two_points() {
    let square = PolylineEntity::new(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
        true,
    );
    let mut h = Harness::with_entities([square.into()]);
    h.enter("BR");
    h.click_world(p(50.0, 0.0));
    h.enter("@");
    assert_eq!(
        h.log().last().unwrap(),
        "A closed polyline has no ends: it needs two different break points."
    );
    h.enter("100,50");
    assert_eq!(
        h.entities(),
        vec![
            PolylineEntity::new(
                vec![
                    p(100.0, 50.0),
                    p(100.0, 100.0),
                    p(0.0, 100.0),
                    p(0.0, 0.0),
                    p(50.0, 0.0)
                ],
                false
            )
            .into()
        ]
    );
}

#[test]
fn escape_or_enter_leaves_the_drawing_alone() {
    let mut h = one_line();
    let revision = h.session.drawing().revision();
    h.enter("BR");
    h.click_world(p(30.0, 0.0));
    h.escape();
    assert_eq!(h.prompt(), "Command:");
    h.enter("BR");
    h.click_world(p(30.0, 0.0));
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(
        h.session
            .overlay()
            .iter()
            .all(|item| !matches!(item, OverlayItem::Highlight(_)))
    );
}

/// The second point goes through the same acquisition as any other: it
/// snaps to the object's own end, which is how an end is taken off exactly.
#[test]
fn the_second_point_snaps_to_the_objects_own_endpoint() {
    let mut h = Harness::with_entities([line(0.0, 0.0, 100.0, 0.0), line(40.0, -30.0, 40.0, 30.0)]);
    h.enter("BR");
    h.click_world(p(70.0, 0.0));
    h.click_world(p(98.0, 3.0)); // near the end at (100, 0)
    assert_eq!(h.entities()[0], line(0.0, 0.0, 70.0, 0.0));
    assert_eq!(
        h.entities().len(),
        2,
        "nothing is left beyond the snapped end"
    );
}
