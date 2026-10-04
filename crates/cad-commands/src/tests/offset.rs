//! OFFSET for lines and circles (#96).

use std::f64::consts::PI;

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line() -> Entity {
    LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into()
}

fn circle() -> Entity {
    CircleEntity::new(p(0.0, 100.0), 40.0).into()
}

/// A line along the X axis and a circle above it.
fn scene() -> Harness {
    Harness::with_entities([line(), circle()])
}

#[test]
fn the_prompts_walk_through_distance_object_and_side() {
    let mut h = scene();
    h.enter("O");
    assert_eq!(h.prompt(), "Specify offset distance:");
    h.enter("10");
    assert_eq!(h.prompt(), "Select object to offset:");
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.prompt(), "Specify point on side to offset:");
    h.click_world(p(50.0, 30.0));
    assert_eq!(h.prompt(), "Select object to offset:", "it repeats");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
}

/// Both sides of a line.
#[test]
fn a_line_is_offset_to_the_side_that_is_picked() {
    let mut h = scene();
    h.enter_all(&["OFFSET", "10"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(20.0, 35.0));
    h.click_world(p(50.0, 0.0));
    h.click_world(p(80.0, -3.0));
    h.enter("");

    let entities = h.entities();
    assert_eq!(entities.len(), 4);
    assert_eq!(
        entities[2],
        LineEntity::new(p(0.0, 10.0), p(100.0, 10.0)).into()
    );
    assert_eq!(
        entities[3],
        LineEntity::new(p(0.0, -10.0), p(100.0, -10.0)).into()
    );
    assert_eq!(entities[0], line(), "the source is untouched");
}

/// Both directions on a circle.
#[test]
fn a_circle_is_offset_outwards_or_inwards() {
    let mut h = scene();
    h.enter_all(&["O", "15"]);
    h.click_world(p(40.0, 100.0));
    h.click_world(p(90.0, 100.0)); // outside
    h.click_world(p(40.0, 100.0));
    h.click_world(p(5.0, 100.0)); // inside
    h.enter("");

    let entities = h.entities();
    assert_eq!(entities[2], CircleEntity::new(p(0.0, 100.0), 55.0).into());
    assert_eq!(entities[3], CircleEntity::new(p(0.0, 100.0), 25.0).into());
}

/// An inward circle offset that would give a non-positive radius is rejected.
#[test]
fn an_inward_offset_that_would_leave_no_circle_is_refused() {
    let mut h = scene();
    h.enter_all(&["O", "40"]);
    h.click_world(p(40.0, 100.0));
    h.click_world(p(0.0, 100.0));
    assert_eq!(h.log().last().unwrap(), "The offset would leave no circle.");
    assert_eq!(h.prompt(), "Select object to offset:");
    assert_eq!(h.entities().len(), 2);

    // Outwards by the same distance is fine.
    h.click_world(p(40.0, 100.0));
    h.click_world(p(200.0, 100.0));
    assert_eq!(
        h.entities()[2],
        CircleEntity::new(p(0.0, 100.0), 80.0).into()
    );
}

/// Arcs and polylines produce a clear "not supported yet" message.
#[test]
fn arcs_and_polylines_are_declined_by_name() {
    let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
    let polyline: Entity =
        PolylineEntity::new(vec![p(-100.0, -50.0), p(100.0, -50.0)], false).into();
    let mut h = Harness::with_entities([arc, polyline]);
    h.enter_all(&["O", "5"]);

    h.click_world(p(0.0, 50.0));
    assert_eq!(h.log().last().unwrap(), "OFFSET does not support arcs yet.");
    assert_eq!(h.prompt(), "Select object to offset:");
    h.click_world(p(0.0, -50.0));
    assert_eq!(
        h.log().last().unwrap(),
        "OFFSET does not support polylines yet."
    );
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn a_side_point_on_the_object_is_asked_again() {
    let mut h = scene();
    h.enter_all(&["O", "10"]);
    h.click_world(p(50.0, 0.0));
    h.enter("70,0");
    assert_eq!(
        h.log().last().unwrap(),
        "That point is on the object: pick a point to one side."
    );
    assert_eq!(h.prompt(), "Specify point on side to offset:");
    h.enter("70,1");
    assert_eq!(h.entities().len(), 3);
}

#[test]
fn the_distance_must_be_positive_and_is_remembered_for_the_session() {
    let mut h = scene();
    h.enter("O");
    for bad in ["0", "-5"] {
        h.enter(bad);
        assert_eq!(
            h.log().last().unwrap(),
            "The offset distance must be greater than zero."
        );
        assert_eq!(h.prompt(), "Specify offset distance:");
    }
    h.enter_all(&["7.5", ""]);
    assert_eq!(h.session.settings().offset_distance, Some(7.5));

    h.enter("O");
    assert_eq!(h.prompt(), "Specify offset distance <7.5>:");
    h.enter("");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 50.0));
    assert_eq!(
        h.entities()[2],
        LineEntity::new(p(0.0, 7.5), p(100.0, 7.5)).into()
    );
}

#[test]
fn enter_without_any_distance_ends_the_command() {
    let mut h = scene();
    h.enter_all(&["O", ""]);
    assert_eq!(h.prompt(), "Command:");
}

/// Transient preview of the result.
#[test]
fn the_result_is_previewed_on_the_side_of_the_cursor_without_touching_the_document() {
    let mut h = scene();
    h.enter_all(&["O", "10"]);
    let id = h.ids()[0];
    h.click_world(p(50.0, 0.0));
    let revision = h.session.drawing().revision();

    h.move_to_world(p(30.0, 60.0));
    let overlay = h.session.overlay();
    assert!(
        overlay.contains(&OverlayItem::Highlight(id)),
        "the chosen object is marked"
    );
    assert!(overlay.contains(&OverlayItem::PreviewEntity(
        LineEntity::new(p(0.0, 10.0), p(100.0, 10.0)).into()
    )));
    h.move_to_world(p(30.0, -60.0));
    assert!(h.session.overlay().contains(&OverlayItem::PreviewEntity(
        LineEntity::new(p(0.0, -10.0), p(100.0, -10.0)).into()
    )));
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn clicks_on_empty_space_and_typed_text_at_the_object_prompt_are_explained() {
    let mut h = scene();
    h.enter_all(&["O", "10"]);
    h.click_world(p(-200.0, -200.0));
    assert_eq!(h.log().last().unwrap(), "No object there.");
    h.enter("50,0");
    assert_eq!(
        h.log().last().unwrap(),
        "Pick an object with the pointer, or press Enter to finish."
    );
    assert_eq!(h.prompt(), "Select object to offset:");
}

#[test]
fn one_offset_command_is_one_undo_step() {
    let mut h = scene();
    h.enter_all(&["O", "10"]);
    for side in [20.0, -20.0] {
        h.click_world(p(50.0, 0.0));
        h.click_world(p(50.0, side));
    }
    h.enter("");
    assert_eq!(h.entities().len(), 4);
    h.undo();
    assert_eq!(h.entities(), vec![line(), circle()]);
}
