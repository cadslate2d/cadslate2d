//! SPLINE (#182): drawing one through points, and what the program does with it.

use cad_core::Point2;
use cad_document::{Entity, SplineEntity};

use crate::snap::SnapMode;
use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn splines(h: &Harness) -> Vec<SplineEntity> {
    h.entities()
        .iter()
        .filter_map(|entity| match entity {
            Entity::Spline(spline) => Some(spline.clone()),
            _ => None,
        })
        .collect()
}

const ARCH: [&str; 4] = ["0,0", "30,60", "70,60", "100,0"];

fn arch() -> Harness {
    let mut h = Harness::new();
    h.enter("SPLINE");
    h.enter_all(&ARCH);
    h.enter("");
    h
}

#[test]
fn a_spline_goes_through_the_points_typed_in_order_and_enter_finishes() {
    let mut h = Harness::new();
    h.enter("SPL");
    assert_eq!(h.prompt(), "Specify first point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify next point or [Undo]:");
    h.enter_all(&["30,60", "70,60", "100,0"]);
    let dirty = h.enter("");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.prompt(), "Command:");
    let spline = &splines(&h)[0];
    assert_eq!(spline.degree, 3);
    assert_eq!(spline.fit.len(), 4);
    assert_eq!(spline.control.len(), 4);
    let geometry = spline.spline();
    assert!(geometry.start_point().distance_to(p(0.0, 0.0)) < 1e-9);
    assert!(geometry.end_point().distance_to(p(100.0, 0.0)) < 1e-9);
    assert!(geometry.distance_to_point(p(30.0, 60.0)) < 0.1);
    assert!(h.session.ui_state().can_undo);
}

#[test]
fn undo_takes_the_last_point_back_and_too_few_points_make_nothing() {
    let mut h = Harness::new();
    h.enter_all(&["SPLINE", "0,0", "50,50", "100,0", "U", "100,20", ""]);
    let spline = &splines(&h)[0];
    assert_eq!(spline.fit, vec![p(0.0, 0.0), p(50.0, 50.0), p(100.0, 20.0)]);
    let mut h = Harness::new();
    h.enter_all(&["SPLINE", "0,0", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert!(splines(&h).is_empty());
    h.enter_all(&["SPLINE", "0,0", "0,0"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The point is the same as the last."
    );
    h.escape();
}

#[test]
fn the_preview_is_the_spline_the_next_point_would_make() {
    let mut h = Harness::new();
    h.enter_all(&["SPLINE", "0,0", "30,60"]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(70.0, 60.0));
    let previewed = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Spline(spline)) => Some(spline),
        _ => None,
    });
    let previewed = previewed.expect("a spline is previewed");
    assert_eq!(previewed.fit.len(), 3);
    assert_eq!(h.session.drawing().revision(), revision);
    h.click_world(p(70.0, 60.0));
    h.enter("");
    assert_eq!(splines(&h)[0], previewed);
}

#[test]
fn it_is_picked_on_its_curve_and_snaps_at_its_ends_and_middle_not_its_chords() {
    let mut h = arch();
    let ids = h.ids();
    // Near the second fit point, which is on the curve.
    h.click_world(p(30.0, 60.0));
    assert_eq!(h.session.selection().iter().collect::<Vec<_>>(), ids);
    h.escape();
    h.click_world(p(50.0, 20.0));
    assert!(h.session.selection().is_empty(), "under the arch is empty");
    h.escape();
    h.enter("L");
    h.move_to_world(p(99.6, 0.4));
    let snap = h.session.snap().unwrap();
    assert_eq!(snap.mode, SnapMode::Endpoint);
    assert!(snap.point.distance_to(p(100.0, 0.0)) < 1e-9);
    // Somewhere along the arch there is no endpoint, only Nearest.
    h.move_to_world(p(15.0, 35.0));
    let mode = h.session.snap().map(|snap| snap.mode);
    assert!(matches!(mode, None | Some(SnapMode::Nearest)), "{mode:?}");
    h.escape();
}

#[test]
fn it_moves_turns_mirrors_scales_and_stretches_by_its_fit_points() {
    let mut h = arch();
    let all = |h: &mut Harness| h.select_box(p(-300.0, -300.0), p(300.0, 300.0));
    all(&mut h);
    h.enter_all(&["M", "0,0", "0,100"]);
    assert_eq!(splines(&h)[0].fit[0], p(0.0, 100.0));
    all(&mut h);
    h.enter_all(&["RO", "0,100", "90"]);
    assert!(splines(&h)[0].fit[3].distance_to(p(0.0, 200.0)) < 1e-9);
    all(&mut h);
    h.enter_all(&["SC", "0,100", "0.5"]);
    assert!(splines(&h)[0].fit[3].distance_to(p(0.0, 150.0)) < 1e-9);
    all(&mut h);
    h.enter_all(&["MI", "0,0", "0,1", "Y"]);
    // Turned and halved about (0, 100), the second point was at (-30, 115); mirrored, it is at (30, 115).
    assert!(splines(&h)[0].fit[1].distance_to(p(30.0, 115.0)) < 1e-9);
    // STRETCH: the fit points in the window move and the curve re-fits.
    let mut h = arch();
    h.enter_all(&["S", "60,-10", "110,70", "0,0", "0,20"]);
    let stretched = &splines(&h)[0];
    assert_eq!(stretched.fit[3], p(100.0, 20.0));
    assert_eq!(stretched.fit[2], p(70.0, 80.0));
    assert_eq!(stretched.fit[0], p(0.0, 0.0));
    assert!(stretched.spline().end_point().distance_to(p(100.0, 20.0)) < 1e-9);
}

#[test]
fn grips_are_its_fit_points_and_dragging_one_refits() {
    let h = arch();
    let entity = h.entities()[0].clone();
    assert_eq!(entity.grips().len(), 4);
    let moved = entity.grip_moved(1, p(30.0, 90.0)).unwrap();
    let Entity::Spline(moved) = moved else {
        panic!()
    };
    assert_eq!(moved.fit[1], p(30.0, 90.0));
    assert!(moved.spline().distance_to_point(p(30.0, 90.0)) < 0.1);
    assert!(entity.grip_moved(4, p(0.0, 0.0)).is_none());
}

#[test]
fn what_it_refuses_says_why_and_it_cuts_other_things() {
    let mut h = arch();
    h.enter_all(&["L", "50,-20", "50,100", ""]);
    let refused = |h: &Harness, what: &str| {
        assert!(
            h.log().iter().any(|line| line.contains("spline")
                && (line.contains("cannot be") || line.contains("does not support"))),
            "{what}: {:?}",
            h.log()
        );
    };
    h.enter("TR");
    h.click_world(p(50.0, -10.0));
    h.enter("");
    h.click_world(p(30.0, 60.0));
    refused(&h, "trim");
    h.escape();
    h.enter("BR");
    h.click_world(p(30.0, 60.0));
    h.click_world(p(70.0, 60.0));
    refused(&h, "break");
    h.escape();
    h.enter_all(&["OFFSET", "5"]);
    h.click_world(p(30.0, 60.0));
    h.click_world(p(30.0, 80.0));
    refused(&h, "offset");
    h.escape();
    // As a cutting edge: the line trimmed at the arch's top, to the chord tolerance.
    h.enter("TR");
    h.click_world(p(30.0, 60.0));
    h.enter("");
    h.click_world(p(50.0, 90.0));
    h.enter("");
    let line = h.lines()[0];
    let top = if line.start.y > line.end.y {
        line.start
    } else {
        line.end
    };
    assert!(top.y > 60.0 && top.y < 70.0, "trimmed at {top:?}");
    assert!(
        splines(&h)[0].spline().distance_to_point(top) < 0.05,
        "on the curve"
    );
}

#[test]
fn a_closed_spline_bounds_a_hatch() {
    let mut h = Harness::new();
    h.enter_all(&["SPLINE", "0,0", "60,30", "0,60", "-60,30", "0,0", ""]);
    assert!(splines(&h)[0].is_closed());
    h.enter_all(&["HATCH", "P", "SOLID", "0,30"]);
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    assert!(h.entities().iter().any(|e| matches!(e, Entity::Hatch(_))));
}
