//! LENGTHEN (#201): a line, an arc, and an open polyline changed at the
//! end nearer the pick by a delta, a percentage, a total, an angle, or
//! the pointer; the rest refused; one undo each.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn drawing() -> Harness {
    let mut h = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        ArcEntity::new(p(200.0, 0.0), 20.0, 0.0, FRAC_PI_2).into(),
        PolylineEntity::with_bulges(
            vec![p(0.0, 100.0), p(50.0, 100.0), p(50.0, 150.0)],
            vec![0.0, 1.0, 0.0],
            false,
        )
        .into(),
        CircleEntity::new(p(200.0, 100.0), 10.0).into(),
    ]);
    h.enter_all(&["ZOOM", "E"]);
    h
}

#[test]
fn lengthen_measures_a_pick_and_changes_by_a_delta_at_the_end_nearer_the_pick() {
    let mut h = drawing();
    h.enter("LENGTHEN");
    assert_eq!(
        h.prompt(),
        "Select an object to measure or [DElta/Percent/Total/DYnamic]:"
    );
    h.click_world(p(50.0, 0.0));
    assert_eq!(last(&h), "Current length: 100.0000");
    h.click_world(p(214.0, 14.0));
    assert_eq!(
        last(&h),
        "Current length: 31.4159, included angle: 90.0000°"
    );
    h.click_world(p(210.0, 100.0));
    assert_eq!(last(&h), "A circle cannot be lengthened.");
    h.enter("DE");
    assert_eq!(h.prompt(), "Enter delta length or [Angle]:");
    h.enter("25");
    assert_eq!(h.prompt(), "Select an object to change:");
    // The line's far end, then its near end.
    let dirty = h.click_world(p(95.0, 0.0));
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.lines()[0], LineEntity::new(p(0.0, 0.0), p(125.0, 0.0)));
    h.click_world(p(5.0, 0.0));
    assert_eq!(h.lines()[0], LineEntity::new(p(-25.0, 0.0), p(125.0, 0.0)));
    // The arc at its end, the polyline at its bulged end.
    h.click_world(p(200.0, 20.0));
    let Entity::Arc(arc) = &h.entities()[1] else {
        panic!("an arc");
    };
    assert!((arc.end_angle - (FRAC_PI_2 + 25.0 / 20.0)).abs() < 1e-9);
    h.click_world(p(50.0, 150.0));
    let Entity::Polyline(polyline) = &h.entities()[2] else {
        panic!("a polyline");
    };
    assert!(
        (polyline.path().length() - (50.0 + 25.0 * PI + 25.0)).abs() < 1e-9,
        "{}",
        polyline.path().length()
    );
    h.click_world(p(210.0, 100.0));
    assert_eq!(last(&h), "A circle cannot be lengthened.");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    // Each change its own step within the command; one undo for the command.
    h.undo();
    assert_eq!(h.lines()[0], LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)));
}

#[test]
fn percent_total_and_angle_set_the_length_and_the_refusals_say_why() {
    let mut h = drawing();
    h.enter_all(&["LEN", "P", "0"]);
    assert_eq!(last(&h), "The amount must be a positive number.");
    h.enter("50");
    h.click_world(p(95.0, 0.0));
    assert_eq!(h.lines()[0], LineEntity::new(p(0.0, 0.0), p(50.0, 0.0)));
    h.enter("");
    h.enter_all(&["LEN", "T", "80"]);
    h.click_world(p(5.0, 0.0));
    assert_eq!(h.lines()[0], LineEntity::new(p(-30.0, 0.0), p(50.0, 0.0)));
    h.enter("");
    h.enter_all(&["LEN", "DE", "A"]);
    assert_eq!(h.prompt(), "Enter delta angle:");
    h.enter("90");
    h.click_world(p(200.0, 20.0));
    let Entity::Arc(arc) = &h.entities()[1] else {
        panic!("an arc");
    };
    assert!((arc.end_angle - PI).abs() < 1e-9);
    h.click_world(p(50.0, 0.0));
    assert_eq!(last(&h), "Only an arc has an angle to change.");
    h.enter("");
    h.enter_all(&["LEN", "T", "A", "360"]);
    h.click_world(p(200.0, 20.0));
    assert_eq!(last(&h), "An arc cannot reach its full circle.");
    h.enter("");
    h.enter_all(&["LEN", "DE", "-200"]);
    h.click_world(p(50.0, 0.0));
    assert_eq!(last(&h), "That would leave nothing of the object.");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn a_pick_is_previewed_as_it_would_leave_the_object_and_dynamic_follows_the_pointer() {
    let mut h = drawing();
    h.enter_all(&["LEN", "DE", "30"]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(95.0, 0.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Line(line)) if line.end == p(130.0, 0.0)
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.escape();

    h.enter_all(&["LEN", "DY"]);
    assert_eq!(h.prompt(), "Select an object to change:");
    h.click_world(p(95.0, 0.0));
    assert_eq!(h.prompt(), "Specify new end point:");
    // The end follows the pointer's projection on the line's own line.
    h.move_to_world(p(140.0, 30.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Line(line)) if line.end == p(140.0, 0.0)
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter("160,20");
    assert_eq!(h.lines()[0], LineEntity::new(p(0.0, 0.0), p(160.0, 0.0)));
    // An arc's end goes round its circle to the pointer's direction.
    h.click_world(p(200.0, 20.0));
    h.enter("180,0");
    let Entity::Arc(arc) = &h.entities()[1] else {
        panic!("an arc");
    };
    assert!((arc.end_angle - PI).abs() < 1e-9);
    h.enter("");
    h.undo();
    assert_eq!(h.lines()[0], LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)));
}
