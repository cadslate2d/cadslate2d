//! ELLIPSE (#181): drawing one, and what the rest of the program does with it.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::{Point2, Vec2};
use cad_document::{EllipseEntity, Entity};

use crate::snap::SnapMode;
use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn ellipses(h: &Harness) -> Vec<EllipseEntity> {
    h.entities()
        .iter()
        .filter_map(|entity| match entity {
            Entity::Ellipse(ellipse) => Some(*ellipse),
            _ => None,
        })
        .collect()
}

fn near(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-9
}

#[test]
fn an_ellipse_by_the_ends_of_one_axis_and_the_other_half_axis() {
    let mut h = Harness::new();
    h.enter("ELLIPSE");
    assert_eq!(
        h.prompt(),
        "Specify axis endpoint of ellipse or [Arc/Center]:"
    );
    h.enter("-50,0");
    assert_eq!(h.prompt(), "Specify other endpoint of axis:");
    h.enter("50,0");
    assert_eq!(h.prompt(), "Specify distance to other axis:");
    let dirty = h.enter("20");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.prompt(), "Command:");
    let ellipse = ellipses(&h)[0];
    assert_eq!(ellipse.center, p(0.0, 0.0));
    assert_eq!(ellipse.major, Vec2::new(50.0, 0.0));
    assert_eq!(ellipse.ratio, 0.4);
    assert!(ellipse.is_full());
    assert!(h.session.ui_state().can_undo);
}

#[test]
fn the_longer_axis_is_always_the_major_one_and_a_point_gives_the_distance() {
    let mut h = Harness::new();
    // The other half-axis, 80, is longer than the 50 of the first: the axes trade.
    h.enter_all(&["EL", "-50,0", "50,0", "80"]);
    let ellipse = ellipses(&h)[0];
    assert!(
        near(Point2::ORIGIN + ellipse.major, p(0.0, 80.0)),
        "{:?}",
        ellipse.major
    );
    assert!((ellipse.ratio - 50.0 / 80.0).abs() < 1e-12);
    // By a point: its distance from the centre.
    let mut h = Harness::new();
    h.enter_all(&["EL", "0,0", "100,0", "50,25"]);
    let ellipse = ellipses(&h)[0];
    assert_eq!(ellipse.center, p(50.0, 0.0));
    assert!((ellipse.ratio - 0.5).abs() < 1e-12);
}

#[test]
fn by_the_centre_and_an_axis_end() {
    let mut h = Harness::new();
    h.enter_all(&["ELLIPSE", "C"]);
    assert_eq!(h.prompt(), "Specify center of ellipse:");
    h.enter_all(&["10,10", "40,10", "15"]);
    let ellipse = ellipses(&h)[0];
    assert_eq!(ellipse.center, p(10.0, 10.0));
    assert_eq!(ellipse.major, Vec2::new(30.0, 0.0));
    assert_eq!(ellipse.ratio, 0.5);
}

#[test]
fn an_arc_of_an_ellipse_by_its_parameters() {
    let mut h = Harness::new();
    h.enter_all(&["ELLIPSE", "A", "-50,0", "50,0", "25"]);
    assert_eq!(h.prompt(), "Specify start parameter:");
    h.enter("0");
    assert_eq!(h.prompt(), "Specify end parameter:");
    h.enter("90");
    let ellipse = ellipses(&h)[0];
    assert!(!ellipse.is_full());
    let geometry = ellipse.ellipse();
    assert!(near(geometry.start_point(), p(50.0, 0.0)));
    assert!(near(geometry.end_point(), p(0.0, 25.0)));
    assert!((geometry.sweep - FRAC_PI_2).abs() < 1e-12);
    // By points: the parameter of the point's direction, not its angle.
    let mut h = Harness::new();
    h.enter_all(&["EL", "A", "-50,0", "50,0", "25", "50,0", "0,-25"]);
    let geometry = ellipses(&h)[0].ellipse();
    assert!((geometry.sweep - 3.0 * FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn a_degenerate_axis_is_refused_and_escape_leaves_nothing() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "0,0", "0,0"]);
    assert_eq!(h.prompt(), "Specify other endpoint of axis:");
    assert_eq!(
        h.log().last().unwrap(),
        "The axis endpoints must be different."
    );
    h.enter_all(&["100,0", "0"]);
    assert_eq!(h.prompt(), "Specify distance to other axis:");
    assert_eq!(h.log().last().unwrap(), "The other axis must have length.");
    h.escape();
    assert!(ellipses(&h).is_empty());
}

#[test]
fn the_preview_shows_the_ellipse_the_click_would_make() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0"]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(10.0, 20.0));
    let previewed = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Ellipse(ellipse)) => Some(ellipse),
        _ => None,
    });
    let previewed = previewed.expect("an ellipse is previewed");
    assert!((previewed.ratio - p(10.0, 20.0).distance_to(Point2::ORIGIN) / 50.0).abs() < 1e-12);
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "a preview commits nothing"
    );
    h.click_world(p(10.0, 20.0));
    assert_eq!(ellipses(&h)[0], previewed);
}

#[test]
fn it_is_picked_on_its_curve_and_snapped_at_its_centre_and_quadrants() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    let ids = h.ids();
    // On the curve at 45° of parameter: (35.36, 17.68).
    h.click_world(p(35.36, 17.68));
    assert_eq!(h.session.selection().iter().collect::<Vec<_>>(), ids);
    h.escape();
    // Not inside it.
    h.click_world(p(10.0, 5.0));
    assert!(h.session.selection().is_empty());
    h.escape();
    h.enter("L");
    h.move_to_world(p(0.3, 25.4));
    let snap = h.session.snap().unwrap();
    assert_eq!(snap.mode, SnapMode::Quadrant);
    assert!(near(snap.point, p(0.0, 25.0)));
    h.move_to_world(p(35.36, 17.68));
    h.move_to_world(p(0.4, 0.3));
    assert_eq!(h.session.snap().unwrap().mode, SnapMode::Center);
    assert_eq!(h.session.snap().unwrap().point, Point2::ORIGIN);
    // Nothing snaps to a chord's end: at 30° of parameter there is no endpoint.
    h.move_to_world(p(43.4, 12.6));
    let mode = h.session.snap().map(|snap| snap.mode);
    // Hovering the curve offers its centre, as a circle's does; never a chord's end.
    assert!(
        matches!(mode, None | Some(SnapMode::Nearest | SnapMode::Center)),
        "{mode:?}"
    );
    h.escape();
}

#[test]
fn it_moves_turns_mirrors_scales_and_stretches_whole() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    let all = |h: &mut Harness| h.select_box(p(-300.0, -300.0), p(300.0, 300.0));
    all(&mut h);
    h.enter_all(&["RO", "0,0", "90"]);
    let turned = ellipses(&h)[0];
    assert!(
        near(Point2::ORIGIN + turned.major, p(0.0, 50.0)),
        "{:?}",
        turned.major
    );
    all(&mut h);
    h.enter_all(&["M", "0,0", "100,0"]);
    assert_eq!(ellipses(&h)[0].center, p(100.0, 0.0));
    all(&mut h);
    h.enter_all(&["SC", "100,0", "2"]);
    assert!((ellipses(&h)[0].major.length() - 100.0).abs() < 1e-9);
    assert_eq!(ellipses(&h)[0].ratio, 0.5);
    all(&mut h);
    h.enter_all(&["MI", "0,0", "0,1", "Y"]);
    assert_eq!(ellipses(&h)[0].center, p(-100.0, 0.0));
    // STRETCH: a crossing window over part of the curve moves nothing — an
    // ellipse is held whole or not at all — and one over all of it moves it whole.
    h.enter_all(&["S", "-60,90", "-10,120", "0,0", "5,5"]);
    assert_eq!(ellipses(&h)[0].center, p(-100.0, 0.0));
    h.enter_all(&["S", "-160,-110", "-40,110", "0,0", "5,5"]);
    assert_eq!(ellipses(&h)[0].center, p(-95.0, 5.0));
}

#[test]
fn grips_move_the_centre_and_the_axes() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    let entity = h.entities()[0].clone();
    let grips = entity.grips();
    assert_eq!(grips.len(), 5);
    let moved = entity.grip_moved(0, p(10.0, 10.0)).unwrap();
    assert_eq!(moved.kind_name(), "ellipse");
    let Entity::Ellipse(moved) = moved else {
        panic!()
    };
    assert_eq!(moved.center, p(10.0, 10.0));
    // The major-axis end at (50, 0) dragged to (0, 80): the long axis turns and
    // grows; the short one keeps its 25.
    let Entity::Ellipse(moved) = entity.grip_moved(1, p(0.0, 80.0)).unwrap() else {
        panic!()
    };
    assert!(near(Point2::ORIGIN + moved.major, p(0.0, 80.0)));
    assert!((moved.ratio - 25.0 / 80.0).abs() < 1e-12);
    // The minor-axis end at (0, 25) dragged out to (0, 40).
    let Entity::Ellipse(moved) = entity.grip_moved(2, p(0.0, 40.0)).unwrap() else {
        panic!()
    };
    assert!((moved.ratio - 0.8).abs() < 1e-12);
    assert!(
        entity.grip_moved(1, p(0.0, 0.0)).is_none(),
        "an axis of no length"
    );
}

#[test]
fn break_and_trim_cut_it_by_parameter_and_it_cuts_other_things() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    // BREAK between the top and the right: the quarter in between goes.
    h.enter("BR");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 25.0));
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    let arc = ellipses(&h)[0].ellipse();
    assert!(!arc.is_full());
    assert!(
        near(arc.start_point(), p(0.0, 25.0)),
        "{:?}",
        arc.start_point()
    );
    assert!(near(arc.end_point(), p(50.0, 0.0)));
    assert!((arc.sweep - 3.0 * FRAC_PI_2).abs() < 1e-9);
    h.undo();
    // TRIM the ellipse with a vertical line through its middle: the right half goes.
    h.enter_all(&["L", "0,-40", "0,40", ""]);
    h.enter("TR");
    h.click_world(p(0.0, 30.0));
    h.enter("");
    h.click_world(p(50.0, 0.0));
    h.enter("");
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    let half = ellipses(&h)[0].ellipse();
    assert!((half.sweep - PI).abs() < 1e-6, "sweep {}", half.sweep);
    assert!(
        near(half.start_point(), p(0.0, 25.0))
            || half.start_point().distance_to(p(0.0, 25.0)) < 1e-3
    );
    // The ellipse as a cutting edge: the line trimmed at it, to a third of a
    // thousandth of the major axis.
    h.undo();
    h.enter("TR");
    h.click_world(p(50.0, 0.0));
    h.enter("");
    h.click_world(p(0.0, 35.0));
    h.enter("");
    let line = h.lines()[0];
    assert!(
        line.end.distance_to(p(0.0, 25.0)) < 0.02 || line.start.distance_to(p(0.0, 25.0)) < 0.02
    );
}

#[test]
fn what_it_refuses_says_why() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    h.enter_all(&["L", "100,-40", "100,40", ""]);
    let refused = |h: &Harness, what: &str| {
        assert!(
            h.log()
                .iter()
                .any(|line| line.contains("ellipse cannot be")),
            "{what}: {:?}",
            h.log()
        );
    };
    h.enter_all(&["OFFSET", "5"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(70.0, 0.0));
    refused(&h, "offset");
    h.escape();
    h.enter("F");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(100.0, 0.0));
    refused(&h, "fillet");
    h.escape();
    h.enter("EX");
    h.click_world(p(100.0, 0.0));
    h.enter("");
    h.click_world(p(50.0, 0.0));
    refused(&h, "extend");
    h.escape();
    h.select_box(p(-300.0, -300.0), p(300.0, 300.0));
    h.enter("X");
    refused(&h, "explode");
    assert_eq!(ellipses(&h).len(), 1);
}

#[test]
fn a_whole_ellipse_bounds_a_hatch() {
    let mut h = Harness::new();
    h.enter_all(&["EL", "-50,0", "50,0", "25"]);
    h.enter_all(&["HATCH", "P", "SOLID", "0,0"]);
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    let hatched = h
        .entities()
        .iter()
        .any(|entity| matches!(entity, Entity::Hatch(_)));
    assert!(hatched);
}
