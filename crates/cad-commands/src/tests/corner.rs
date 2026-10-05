//! FILLET (#99) and CHAMFER (#100) through the session.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_document::{CircleEntity, Entity, LineEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// Two lines that would meet at the origin at a right angle, both stopping
/// short of it.
fn l_shape() -> Harness {
    Harness::with_entities([line(10.0, 0.0, 100.0, 0.0), line(0.0, 10.0, 0.0, 80.0)])
}

fn pick_both(h: &mut Harness) {
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 50.0));
}

// ── FILLET ──

#[test]
fn fillet_with_the_default_zero_radius_makes_a_sharp_corner() {
    let mut h = l_shape();
    let ids = h.ids();
    h.enter("F");
    assert_eq!(h.log().last().unwrap(), "Current fillet radius: 0");
    assert_eq!(h.prompt(), "Select first object or [Radius]:");
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.prompt(), "Select second object:");
    h.click_world(p(0.0, 50.0));

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.ids(), ids, "two lines, same IDs, no arc");
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 100.0, 0.0), line(0.0, 0.0, 0.0, 80.0)]
    );
}

/// The resulting arc is tangent to both lines within tolerance.
#[test]
fn fillet_with_a_radius_adds_a_tangent_arc_and_trims_both_lines() {
    let mut h = l_shape();
    h.enter_all(&["FILLET", "R"]);
    assert_eq!(h.prompt(), "Specify fillet radius <0>:");
    h.enter("20");
    assert_eq!(h.prompt(), "Select first object or [Radius]:");
    pick_both(&mut h);

    let entities = h.entities();
    assert_eq!(entities.len(), 3);
    assert_eq!(entities[0], line(20.0, 0.0, 100.0, 0.0));
    assert_eq!(entities[1], line(0.0, 20.0, 0.0, 80.0));
    let Entity::Arc(arc) = &entities[2] else {
        panic!("the third entity is the fillet arc");
    };
    let arc = arc.arc();
    assert!(arc.center.distance_to(p(20.0, 20.0)) < 1e-9);
    assert!((arc.radius - 20.0).abs() < 1e-9 && (arc.sweep - FRAC_PI_2).abs() < 1e-9);
    // Tangent: each radius to a joint is perpendicular to the line it meets.
    for (joint, along) in [(p(20.0, 0.0), p(1.0, 0.0)), (p(0.0, 20.0), p(0.0, 1.0))] {
        assert!(arc.distance_to_point(joint) < 1e-9);
        let radius = joint - arc.center;
        assert!((radius.x * along.x + radius.y * along.y).abs() < 1e-9);
    }
}

#[test]
fn the_fillet_radius_is_remembered_for_the_session() {
    let mut h = l_shape();
    h.enter_all(&["F", "R", "15", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.settings().fillet_radius, 15.0);

    h.enter("F");
    assert_eq!(h.log().last().unwrap(), "Current fillet radius: 15");
    h.enter("R");
    assert_eq!(h.prompt(), "Specify fillet radius <15>:");
    h.enter(""); // keep it
    pick_both(&mut h);
    assert_eq!(h.entities()[0], line(15.0, 0.0, 100.0, 0.0));
}

/// Parallel lines and too-large radii are rejected with a message.
#[test]
fn fillet_refuses_parallel_lines_and_radii_that_are_too_large() {
    let mut h = Harness::with_entities([
        line(10.0, 0.0, 100.0, 0.0),
        line(0.0, 10.0, 0.0, 80.0),
        line(10.0, 40.0, 100.0, 40.0),
    ]);
    let before = h.entities();
    let revision = h.session.drawing().revision();

    h.enter_all(&["F", "R", "500"]);
    pick_both(&mut h);
    assert_eq!(
        h.log().last().unwrap(),
        "The radius is too large for these lines."
    );
    assert_eq!(
        h.prompt(),
        "Select first object or [Radius]:",
        "start again"
    );

    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 40.0));
    assert_eq!(h.log().last().unwrap(), "The lines are parallel.");

    h.enter_all(&["R", "-1"]);
    assert_eq!(h.log().last().unwrap(), "The value cannot be negative.");
    h.escape();
    assert_eq!(h.entities(), before);
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn fillet_needs_two_different_objects_and_declines_polylines() {
    let mut h = Harness::new();
    h.enter_all(&[
        "L", "10,0", "100,0", "", "PL", "-100,50", "-50,80", "0,50", "",
    ]);
    h.enter("F");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(60.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "Pick a different object for the second one."
    );
    h.click_world(p(50.0, 0.0));
    h.click_world(p(-50.0, 80.0));
    assert_eq!(
        h.log().last().unwrap(),
        "FILLET does not support polylines yet."
    );
    assert_eq!(h.entities().len(), 2);
}

// ── arcs and circles (#124) ──

#[test]
fn fillet_joins_a_line_to_a_circle_and_leaves_the_circle_whole() {
    let circle: Entity = CircleEntity::new(p(0.0, 40.0), 30.0).into();
    let mut h = Harness::with_entities([line(-200.0, 0.0, 200.0, 0.0), circle.clone()]);
    let ids = h.ids();
    h.enter_all(&["F", "R", "10"]);
    h.click_world(p(60.0, 0.0));
    h.click_world(p(30.0, 40.0)); // the circle's rightmost point

    let entities = h.entities();
    assert_eq!(entities.len(), 3);
    assert_eq!(&h.ids()[..2], &ids[..]);
    assert_eq!(entities[1], circle);
    let reach = 700f64.sqrt();
    let trimmed = h.lines()[0];
    assert!(trimmed.start.distance_to(p(reach, 0.0)) < 1e-9 && trimmed.end == p(200.0, 0.0));
    let Entity::Arc(arc) = &entities[2] else {
        panic!("the third entity is the fillet arc");
    };
    assert!(arc.center.distance_to(p(reach, 10.0)) < 1e-9);
    h.undo();
    assert_eq!(h.entities(), vec![line(-200.0, 0.0, 200.0, 0.0), circle]);
}

#[test]
fn fillet_joins_two_arcs_and_says_when_no_arc_fits() {
    use cad_document::ArcEntity;
    let left: Entity = ArcEntity::new(p(-50.0, 0.0), 30.0, 0.0, std::f64::consts::PI).into();
    let right: Entity = ArcEntity::new(p(50.0, 0.0), 30.0, 0.0, std::f64::consts::PI).into();
    let mut h = Harness::with_entities([left, right]);
    h.enter_all(&["F", "R", "15"]);
    h.click_world(p(-50.0, 30.0));
    h.click_world(p(50.0, 30.0));
    assert_eq!(
        h.log().last().unwrap(),
        "No fillet of that radius fits these objects."
    );
    assert_eq!(h.entities().len(), 2);

    h.enter_all(&["R", "40"]);
    h.click_world(p(-50.0, 30.0));
    h.click_world(p(50.0, 30.0));
    let entities = h.entities();
    assert_eq!(entities.len(), 3);
    let Entity::Arc(joint) = &entities[2] else {
        panic!("the fillet arc");
    };
    assert!(joint.center.distance_to(p(0.0, 2400f64.sqrt())) < 1e-9);
    // Both arcs now end where the fillet touches them.
    for index in [0, 1] {
        let Entity::Arc(kept) = &entities[index] else {
            panic!("an arc stays an arc");
        };
        let kept = kept.arc();
        let nearer = kept
            .start_point()
            .distance_to(joint.center)
            .min(kept.end_point().distance_to(joint.center));
        assert!((nearer - 40.0).abs() < 1e-9);
    }
}

#[test]
fn chamfer_is_for_straight_lines_only() {
    let circle: Entity = CircleEntity::new(p(-100.0, 0.0), 30.0).into();
    let mut h = Harness::with_entities([line(10.0, 0.0, 100.0, 0.0), circle]);
    h.enter("CHA");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(-70.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "CHAMFER joins two straight lines, not circles."
    );
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn a_fillet_is_one_transaction_and_one_undo_step() {
    let mut h = l_shape();
    let before = h.entities();
    h.enter_all(&["F", "R", "20"]);
    pick_both(&mut h);
    assert_eq!(h.entities().len(), 3);
    h.undo();
    assert_eq!(h.entities(), before);
    assert!(!h.session.ui_state().can_undo);
    h.redo();
    assert_eq!(h.entities().len(), 3);
}

#[test]
fn the_first_line_stays_highlighted_while_the_second_is_picked() {
    let mut h = l_shape();
    let first = h.ids()[0];
    h.enter("F");
    h.click_world(p(50.0, 0.0));
    h.move_to_world(p(5.0, 50.0));
    assert!(h.session.overlay().contains(&OverlayItem::Highlight(first)));
}

#[test]
fn the_picked_sides_are_the_ones_kept_when_lines_cross() {
    // A plus sign through the origin.
    let mut h =
        Harness::with_entities([line(-100.0, 0.0, 100.0, 0.0), line(0.0, -80.0, 0.0, 80.0)]);
    h.enter("F");
    h.click_world(p(-60.0, 0.0)); // keep the left arm
    h.click_world(p(0.0, -40.0)); // and the lower arm
    assert_eq!(
        h.entities(),
        vec![line(-100.0, 0.0, 0.0, 0.0), line(0.0, -80.0, 0.0, 0.0)]
    );
}

// ── CHAMFER ──

/// Equal and unequal distances.
#[test]
fn chamfer_cuts_each_line_back_and_joins_them_with_a_line() {
    let mut h = l_shape();
    h.enter("CHA");
    assert_eq!(h.log().last().unwrap(), "Current chamfer distances: 0, 0");
    assert_eq!(h.prompt(), "Select first line or [Distance]:");
    h.enter("D");
    assert_eq!(h.prompt(), "Specify first chamfer distance <0>:");
    h.enter("15");
    assert_eq!(h.prompt(), "Specify second chamfer distance <15>:");
    h.enter("");
    pick_both(&mut h);
    assert_eq!(
        h.entities(),
        vec![
            line(15.0, 0.0, 100.0, 0.0),
            line(0.0, 15.0, 0.0, 80.0),
            line(15.0, 0.0, 0.0, 15.0)
        ]
    );

    // Unequal: the first distance belongs to the first line picked.
    let mut h = l_shape();
    h.enter_all(&["CHAMFER", "D", "30", "5"]);
    pick_both(&mut h);
    assert_eq!(h.entities()[2], line(30.0, 0.0, 0.0, 5.0));

    let mut h = l_shape();
    h.enter_all(&["CHAMFER", "D", "30", "5"]);
    h.click_world(p(0.0, 50.0)); // the vertical line first this time
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.entities()[0], line(5.0, 0.0, 100.0, 0.0));
    assert_eq!(h.entities()[1], line(0.0, 30.0, 0.0, 80.0));
}

#[test]
fn chamfer_with_zero_distances_makes_a_sharp_corner() {
    let mut h = l_shape();
    h.enter("CHA");
    pick_both(&mut h);
    assert_eq!(
        h.entities(),
        vec![line(0.0, 0.0, 100.0, 0.0), line(0.0, 0.0, 0.0, 80.0)]
    );
}

/// Parallel lines and too-large distances are rejected with a message.
#[test]
fn chamfer_refuses_parallel_lines_and_distances_that_are_too_large() {
    let mut h = Harness::with_entities([
        line(10.0, 0.0, 100.0, 0.0),
        line(0.0, 10.0, 0.0, 80.0),
        line(10.0, 40.0, 100.0, 40.0),
    ]);
    let before = h.entities();
    h.enter_all(&["CHA", "D", "10", "90"]);
    pick_both(&mut h);
    assert_eq!(
        h.log().last().unwrap(),
        "The distances are too large for these lines."
    );
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 40.0));
    assert_eq!(h.log().last().unwrap(), "The lines are parallel.");
    h.escape();
    assert_eq!(h.entities(), before);
}

#[test]
fn chamfer_distances_are_remembered_and_a_chamfer_is_one_undo_step() {
    let mut h = l_shape();
    let before = h.entities();
    h.enter_all(&["CHA", "D", "12", "8"]);
    pick_both(&mut h);
    assert_eq!(h.session.settings().chamfer_distances, [12.0, 8.0]);
    h.undo();
    assert_eq!(h.entities(), before);

    h.enter("CHA");
    assert_eq!(h.log().last().unwrap(), "Current chamfer distances: 12, 8");
}
