//! Object snap tracking through the session (#220): a point acquired by
//! resting on it and released the same way, a point on one path, the
//! crossing of two, a distance typed along a path, and its place after
//! object snap and before ORTHO.

use cad_core::Point2;
use cad_document::LineEntity;

use crate::testing::Harness;
use crate::{OverlayItem, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn aimed(h: &Harness) -> Point2 {
    h.session.aim().expect("a cursor").point
}

fn acquired(h: &Harness) -> Vec<Point2> {
    h.session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::AcquiredPoint { at } => Some(at),
            _ => None,
        })
        .collect()
}

/// Two lines, ending at (100, 0) and at (0, 80), with OTRACK on.
fn two_lines() -> Harness {
    let mut h = Harness::with_entities([
        LineEntity::new(p(40.0, 0.0), p(100.0, 0.0)).into(),
        LineEntity::new(p(0.0, 120.0), p(0.0, 80.0)).into(),
    ]);
    h.toggle(Toggle::Otrack);
    h
}

/// Rests the pointer on `at`.
fn rest_on(h: &mut Harness, at: Point2) {
    h.move_to_world(at);
    h.session.pointer_rest();
}

#[test]
fn resting_on_a_snap_point_acquires_it_and_resting_again_lets_it_go() {
    let mut h = two_lines();
    // At the idle prompt there is nothing to track for.
    rest_on(&mut h, p(100.0, 0.0));
    assert!(acquired(&h).is_empty());
    h.enter("L");
    rest_on(&mut h, p(100.5, 0.5));
    assert_eq!(acquired(&h), vec![p(100.0, 0.0)]);
    // Off a snap point, a rest does nothing.
    rest_on(&mut h, p(60.0, 60.0));
    assert_eq!(acquired(&h), vec![p(100.0, 0.0)]);
    rest_on(&mut h, p(100.0, 0.0));
    assert!(acquired(&h).is_empty(), "released");
    // OTRACK off: no acquiring.
    h.toggle(Toggle::Otrack);
    rest_on(&mut h, p(100.0, 0.0));
    assert!(acquired(&h).is_empty());
}

/// A browser repeats a move where the pointer already is when the page
/// around it changes — the tracking chip appearing — and the host times a
/// rest from it. That rest is not the user's: the point stays acquired
/// until the pointer goes away and comes back.
#[test]
fn a_move_where_the_pointer_already_is_does_not_let_the_point_go() {
    let mut h = two_lines();
    h.enter("L");
    rest_on(&mut h, p(100.0, 0.0));
    assert_eq!(acquired(&h), vec![p(100.0, 0.0)]);
    rest_on(&mut h, p(100.0, 0.0));
    assert_eq!(acquired(&h), vec![p(100.0, 0.0)], "the same place again");
    rest_on(&mut h, p(60.0, 60.0));
    rest_on(&mut h, p(100.0, 0.0));
    assert!(acquired(&h).is_empty(), "away and back lets it go");
}

#[test]
fn the_pointer_follows_a_path_and_takes_where_two_cross() {
    let mut h = two_lines();
    h.enter("L");
    rest_on(&mut h, p(100.0, 0.0));
    h.move_to_world(p(200.0, 3.0));
    assert_eq!(aimed(&h), p(200.0, 0.0));
    assert_eq!(
        h.session.ui_state().tracking.as_deref(),
        Some("Tracking: 100.0000 < 0°")
    );
    // Level with one end and under the other: where the paths cross.
    rest_on(&mut h, p(0.0, 80.0));
    h.move_to_world(p(3.0, -2.0));
    assert!(aimed(&h).distance_to(p(0.0, 0.0)) < 1e-9);
    assert_eq!(
        h.session.ui_state().tracking.as_deref(),
        Some("Tracking: intersection")
    );
    // The paths are drawn, and the points they run from marked.
    let lines = h
        .session
        .overlay()
        .into_iter()
        .filter(|item| matches!(item, OverlayItem::TrackingLine { .. }))
        .count();
    assert_eq!((lines, acquired(&h).len()), (2, 2));
    h.click_world(p(3.0, -2.0));
    h.enter_all(&["-50,-50", ""]);
    assert_eq!(h.lines()[2].start, p(0.0, 0.0));
    // The command over, the acquired points go.
    h.enter("L");
    assert!(acquired(&h).is_empty());
}

#[test]
fn a_distance_typed_on_a_path_is_measured_from_its_point() {
    let mut h = two_lines();
    h.enter_all(&["L", "500,500"]);
    rest_on(&mut h, p(100.0, 0.0));
    h.move_to_world(p(150.0, 2.0));
    h.enter("30");
    h.enter("");
    assert_eq!(h.lines()[2].end, p(130.0, 0.0));
}

#[test]
fn object_snap_comes_first_and_tracking_before_ortho() {
    let mut h = two_lines();
    h.enter_all(&["L", "300,300"]);
    rest_on(&mut h, p(100.0, 0.0));
    // On the other line's end, the endpoint, though a path is near.
    h.move_to_world(p(0.5, 80.5));
    assert_eq!(aimed(&h), p(0.0, 80.0));
    // ORTHO from (300, 300) would give (300, 1); tracking gives the path.
    h.toggle(Toggle::Ortho);
    h.move_to_world(p(220.0, 1.0));
    assert_eq!(aimed(&h), p(220.0, 0.0));
    // A revision unchanged: tracking is the overlay's alone.
    let revision = h.session.drawing().revision();
    h.session.pointer_rest();
    h.move_to_world(p(230.0, 1.0));
    assert_eq!(h.session.drawing().revision(), revision);
}
