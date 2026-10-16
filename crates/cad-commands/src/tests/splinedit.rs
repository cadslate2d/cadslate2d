//! SPLINEDIT through the session (#237): each option, the refusals, and
//! one undo for the invocation.

use cad_core::Point2;
use cad_document::{Entity, SplineEntity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn spline(h: &Harness) -> SplineEntity {
    match &h.entities()[0] {
        Entity::Spline(spline) => spline.clone(),
        other => panic!("{other:?} is not a spline"),
    }
}

fn wave() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["SPLINE", "0,0", "100,100", "200,0", "300,100", ""]);
    h
}

#[test]
fn a_spline_is_closed_reversed_and_its_fit_points_edited_as_one_undo() {
    let mut h = wave();
    let original = spline(&h);
    h.enter("SPE");
    assert_eq!(h.prompt(), "Select spline:");
    h.click_world(p(0.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Enter an option or [Close/Open/Fit data/Reverse/Polyline] <eXit>:"
    );
    h.enter("C");
    assert!(spline(&h).is_closed());
    h.enter("C");
    assert!(said(&h, "The spline is closed already."));
    h.enter("O");
    assert!(!spline(&h).is_closed());
    h.enter("R");
    assert_eq!(spline(&h).fit[0], p(300.0, 100.0), "reversed");
    h.enter_all(&["F", "A", "201,1", "250,20"]);
    assert_eq!(spline(&h).fit.len(), 5);
    assert_eq!(
        spline(&h).fit[2],
        p(250.0, 20.0),
        "after the nearest fit point"
    );
    h.enter_all(&["D", "99,99", "", ""]);
    assert_eq!(spline(&h).fit.len(), 4);
    assert_eq!(h.prompt(), "Command:");
    h.undo();
    assert_eq!(spline(&h), original, "one undo");
}

#[test]
fn a_spline_becomes_a_polyline_and_other_objects_are_refused() {
    let mut h = wave();
    h.enter_all(&["L", "0,-50", "100,-50", ""]);
    h.enter("SPLINEDIT");
    h.click_world(p(50.0, -50.0));
    assert!(said(&h, "A line is not a spline."));
    h.click_world(p(0.0, 0.0));
    h.enter("P");
    let Entity::Polyline(polyline) = &h.entities()[0] else {
        panic!("a polyline in the spline's place");
    };
    assert!(polyline.vertices.len() > 10);
    assert_eq!(polyline.vertices[0], p(0.0, 0.0));
}
