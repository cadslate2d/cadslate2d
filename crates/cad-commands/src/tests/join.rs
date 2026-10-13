//! JOIN (#127) through the session. What joins into what is tested in
//! `cad-document/src/joining.rs`; these tests cover the command: selection
//! before or during, the `Close` option, identity, undo, and refusals.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// An arc about the origin, radius 50, between two angles in degrees.
fn arc(from: f64, to: f64) -> Entity {
    ArcEntity::new(p(0.0, 0.0), 50.0, from.to_radians(), to.to_radians()).into()
}

fn last_message(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// Selects everything inside a large window, then runs `lines`.
fn select_all_and(h: &mut Harness, lines: &[&str]) {
    h.select_box(p(-400.0, -400.0), p(400.0, 400.0));
    h.enter_all(lines);
}

#[test]
fn collinear_lines_are_joined_into_one_line_in_one_step() {
    let mut h = Harness::with_entities([
        line(0.0, 0.0, 30.0, 0.0),
        line(70.0, 0.0, 100.0, 0.0),
        line(200.0, 200.0, 250.0, 200.0), // not selected
    ]);
    let ids = h.ids();
    h.enter("J");
    assert_eq!(
        h.prompt(),
        "Select objects to join or [CLose/Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(10.0, 0.0));
    h.click_world(p(90.0, 0.0));
    assert_eq!(last_message(&h), "1 found, 2 total");
    h.enter("");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(last_message(&h), "2 objects joined into 1 line.");
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 100.0, 0.0), line(200.0, 200.0, 250.0, 200.0)]
    );
    assert_eq!(
        h.ids(),
        vec![ids[0], ids[2]],
        "the older line is the result"
    );
    assert!(h.session.selection().is_empty());

    h.undo();
    assert_eq!(h.entities().len(), 3, "one step");
    assert_eq!(h.ids(), ids);
    h.redo();
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn a_selection_made_beforehand_is_joined_at_once() {
    let mut h = Harness::with_entities([arc(0.0, 90.0), arc(90.0, 150.0)]);
    select_all_and(&mut h, &["JOIN"]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(last_message(&h), "2 objects joined into 1 arc.");
    assert_eq!(h.entities(), vec![arc(0.0, 150.0)]);
}

#[test]
fn arcs_that_close_the_circle_become_a_circle() {
    let mut h = Harness::with_entities([arc(0.0, 180.0), arc(180.0, 360.0)]);
    select_all_and(&mut h, &["J"]);
    assert_eq!(last_message(&h), "2 objects joined into 1 circle.");
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(0.0, 0.0), 50.0).into()]
    );
}

#[test]
fn the_close_option_makes_the_circle_an_arc_is_part_of() {
    let mut h = Harness::with_entities([arc(0.0, 90.0)]);
    let ids = h.ids();
    h.enter("J");
    h.click_world(p(50.0 * FRAC_1_SQRT_2, 50.0 * FRAC_1_SQRT_2)); // on the arc, at 45°
    h.enter("CL"); // `C` alone is the crossing polygon of every selection prompt
    assert_eq!(last_message(&h), "1 arc closed into a circle.");
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(0.0, 0.0), 50.0).into()]
    );
    assert_eq!(h.ids(), ids);
    h.undo();
    assert_eq!(h.entities(), vec![arc(0.0, 90.0)]);
}

#[test]
fn arcs_with_a_gap_are_refused_with_a_pointer_to_close() {
    let mut h = Harness::with_entities([arc(0.0, 60.0), arc(120.0, 200.0)]);
    let revision = h.session.drawing().revision();
    select_all_and(&mut h, &["J"]);
    assert_eq!(
        last_message(&h),
        "These arcs are on one circle but do not touch. The Close option makes the whole circle."
    );
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);

    // Close does not mind the gap.
    h.escape();
    h.enter("J");
    h.select_box(p(-400.0, -400.0), p(400.0, 400.0));
    h.enter("Close");
    assert_eq!(last_message(&h), "2 arcs closed into a circle.");
    assert_eq!(h.entities().len(), 1);
}

#[test]
fn lines_and_arcs_meeting_end_to_end_become_a_polyline() {
    // A slot: two lines and two half circles.
    let mut h = Harness::with_entities([
        line(0.0, -20.0, 100.0, -20.0),
        ArcEntity::new(p(100.0, 0.0), 20.0, -FRAC_PI_2, FRAC_PI_2).into(),
        line(100.0, 20.0, 0.0, 20.0),
        ArcEntity::new(p(0.0, 0.0), 20.0, FRAC_PI_2, 3.0 * FRAC_PI_2).into(),
    ]);
    let ids = h.ids();
    select_all_and(&mut h, &["J"]);
    assert_eq!(last_message(&h), "4 objects joined into 1 polyline.");
    let entities = h.entities();
    let [Entity::Polyline(slot)] = entities.as_slice() else {
        panic!("one polyline, got {entities:?}");
    };
    assert!(slot.closed);
    assert_eq!(slot.vertices.len(), 4);
    assert!((slot.bulge(1) - 1.0).abs() < 1e-12 && (slot.bulge(3) - 1.0).abs() < 1e-12);
    assert_eq!(h.ids(), vec![ids[0]]);

    h.undo();
    assert_eq!(h.ids(), ids);
}

#[test]
fn separate_chains_become_separate_polylines_and_the_rest_is_left() {
    let mut h = Harness::with_entities([
        line(0.0, 0.0, 50.0, 0.0),
        line(50.0, 0.0, 50.0, 50.0),
        line(-100.0, -100.0, -50.0, -100.0),
        line(-50.0, -100.0, -50.0, -50.0),
        CircleEntity::new(p(200.0, 200.0), 10.0).into(),
    ]);
    select_all_and(&mut h, &["J"]);
    assert_eq!(last_message(&h), "4 objects joined into 2 polylines.");
    assert_eq!(
        h.entities(),
        vec![
            PolylineEntity::new(vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0)], false).into(),
            PolylineEntity::new(
                vec![p(-100.0, -100.0), p(-50.0, -100.0), p(-50.0, -50.0)],
                false
            )
            .into(),
            CircleEntity::new(p(200.0, 200.0), 10.0).into(),
        ]
    );
}

/// Objects that cannot be joined: a message, and no change.
#[test]
fn objects_that_cannot_be_joined_are_refused_with_a_message() {
    let cases: [(Vec<Entity>, &str); 4] = [
        (
            vec![line(0.0, 0.0, 50.0, 0.0), line(0.0, 20.0, 50.0, 20.0)],
            "Nothing to join: none of these objects meet end to end.",
        ),
        (
            vec![
                CircleEntity::new(p(0.0, 0.0), 50.0).into(),
                line(50.0, 0.0, 90.0, 0.0),
            ],
            "Nothing to join: none of these objects meet end to end.",
        ),
        (
            vec![line(0.0, 0.0, 50.0, 0.0)],
            "Select at least two objects to join.",
        ),
        (
            vec![
                PolylineEntity::new(vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0)], true).into(),
                line(50.0, 50.0, 90.0, 90.0),
            ],
            "Nothing to join: none of these objects meet end to end.",
        ),
    ];
    for (entities, message) in cases {
        let mut h = Harness::with_entities(entities.clone());
        let revision = h.session.drawing().revision();
        select_all_and(&mut h, &["J"]);
        assert_eq!(last_message(&h), message);
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.entities(), entities);
        assert_eq!(h.session.drawing().revision(), revision);
    }
}

#[test]
fn close_refuses_anything_but_arcs_of_one_circle() {
    let mut h = Harness::with_entities([arc(0.0, 90.0), line(0.0, 50.0, -80.0, 50.0)]);
    let before = h.entities();
    h.enter("J");
    h.select_box(p(-400.0, -400.0), p(400.0, 400.0));
    h.enter("CL"); // `C` alone is the crossing polygon of every selection prompt
    assert_eq!(
        last_message(&h),
        "Close makes a circle out of arcs of one circle; select only those."
    );
    assert_eq!(h.entities(), before);
}

#[test]
fn enter_with_nothing_selected_ends_the_command() {
    let mut h = Harness::with_entities([line(0.0, 0.0, 50.0, 0.0)]);
    h.enter_all(&["J", ""]);
    assert_eq!(last_message(&h), "No objects selected.");
    assert_eq!(h.prompt(), "Command:");
}

/// JOIN undoes a BREAK at one point exactly, for every kind that splits.
#[test]
fn join_is_the_inverse_of_breaking_at_a_point() {
    let half_circle: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
    let bent: Entity = PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 80.0)],
        vec![0.0, 0.5],
        false,
    )
    .into();
    let cases = [
        (line(-30.0, 10.0, 90.0, 10.0), p(20.0, 10.0)),
        (half_circle, p(0.0, 50.0)),
        (bent, p(40.0, 0.0)),
    ];
    for (entity, at) in cases {
        let mut h = Harness::with_entities([entity.clone()]);
        h.enter("BR");
        h.click_world(at);
        h.enter("@");
        assert_eq!(h.entities().len(), 2, "{entity:?}");
        select_all_and(&mut h, &["J"]);
        let rejoined = h.entities();
        assert_eq!(rejoined.len(), 1, "{rejoined:?}");
        match (&rejoined[0], &entity) {
            // The polyline keeps the vertex it was broken at.
            (Entity::Polyline(joined), Entity::Polyline(original)) => {
                assert_eq!(joined.vertices.len(), original.vertices.len() + 1);
                assert_eq!(joined.vertices.first(), original.vertices.first());
                assert_eq!(joined.vertices.last(), original.vertices.last());
            }
            (Entity::Arc(joined), Entity::Arc(original)) => {
                assert!((joined.arc().sweep - original.arc().sweep).abs() < 1e-12);
                assert!((joined.arc().start - original.arc().start).abs() < 1e-12);
            }
            (joined, original) => assert_eq!(joined, original),
        }
    }
}
