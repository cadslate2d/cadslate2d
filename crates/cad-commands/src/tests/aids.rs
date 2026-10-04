//! ORTHO and POLAR as the user meets them, and their precedence against
//! typed coordinates and object snaps.

use cad_core::{Point2, ScreenPoint};
use cad_document::{Entity, LineEntity, PolylineEntity};

use crate::testing::Harness;
use crate::{Dirty, Modifiers, OverlayItem, Toggle, TrackKind};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
    LineEntity::new(p(x0, y0), p(x1, y1))
}

fn with(toggle: Toggle) -> Harness {
    let mut h = Harness::new();
    h.toggle(toggle);
    h
}

fn tracking_line(h: &Harness) -> Option<(Point2, Point2)> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::TrackingLine { from, through } => Some((from, through)),
        _ => None,
    })
}

// ── ORTHO (#81) ──

#[test]
fn ortho_constrains_a_clicked_point_to_horizontal_or_vertical() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["L", "0,0"]);
    h.click_world(p(80.0, 20.0));
    h.click_world(p(95.0, 70.0));
    h.enter("");
    assert_eq!(
        h.lines(),
        vec![line(0.0, 0.0, 80.0, 0.0), line(80.0, 0.0, 80.0, 70.0)]
    );
}

/// The precedence, in one place: typed > snap > ortho > free.
#[test]
fn precedence_is_typed_then_snap_then_ortho_then_free() {
    // A target to snap to, away from both axes through the base point.
    let mut h = Harness::new();
    h.enter_all(&["L", "100,50", "150,50", ""]);
    h.toggle(Toggle::Ortho);

    // Typed coordinates are never constrained, absolute or relative.
    h.enter_all(&["L", "0,0", "30,40", "@50<30", ""]);
    assert_eq!(h.lines()[1], line(0.0, 0.0, 30.0, 40.0));
    assert!(h.lines()[2].end.distance_to(p(30.0 + 43.301_270_189, 65.0)) < 1e-6);

    // An object snap beats ORTHO: the exact endpoint, although it is off-axis.
    h.enter_all(&["L", "0,0"]);
    h.click_world(p(98.0, 52.0));
    h.enter("");
    assert_eq!(h.lines()[3], line(0.0, 0.0, 100.0, 50.0));

    // With nothing to snap to, ORTHO constrains the pointer.
    h.enter_all(&["L", "0,200"]);
    h.click_world(p(60.0, 225.0));
    h.enter("");
    assert_eq!(h.lines()[4], line(0.0, 200.0, 60.0, 200.0));

    // With ORTHO off, the pointer is free.
    h.toggle(Toggle::Ortho);
    h.enter_all(&["L", "0,200"]);
    h.click_world(p(60.0, 225.0));
    h.enter("");
    assert_eq!(h.lines()[5], line(0.0, 200.0, 60.0, 225.0));
}

#[test]
fn the_first_point_of_a_command_is_not_constrained() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["L", "0,0", "10,0", "", "L"]);
    h.click_world(p(33.0, 47.0));
    h.click_world(p(90.0, 60.0));
    h.enter("");
    // Free first point; the second is constrained from it.
    assert_eq!(h.lines()[1], line(33.0, 47.0, 90.0, 47.0));
}

#[test]
fn the_preview_the_readout_and_the_click_follow_the_constrained_direction() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["L", "0,0"]);
    h.move_to_world(p(20.0, 75.0));

    assert!(h.session.overlay().contains(&OverlayItem::PreviewSegment {
        start: p(0.0, 0.0),
        end: p(0.0, 75.0)
    }));
    // The crosshair stays with the pointer.
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::Crosshair { at: p(20.0, 75.0) })
    );
    let ui = h.session.ui_state();
    assert_eq!(ui.cursor, Some((0.0, 75.0)));
    assert_eq!(ui.tracking.as_deref(), Some("Ortho: 75.0000 < 90°"));
    assert_eq!(
        h.session.aim().unwrap().track.unwrap().kind,
        TrackKind::Ortho
    );
    assert_eq!(tracking_line(&h), None, "the rubber band already shows it");
}

#[test]
fn direct_distance_follows_the_ortho_direction() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["L", "10,10"]);
    h.move_to_world(p(-40.0, 30.0));
    h.enter_all(&["25", ""]);
    assert_eq!(h.lines(), vec![line(10.0, 10.0, -15.0, 10.0)]);
}

/// F8 toggles mid-command without cancelling.
#[test]
fn f8_takes_effect_mid_command_without_cancelling_it() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0"]);
    h.click_world(p(50.0, 20.0));
    let prompt = h.prompt();

    let dirty = h.toggle(Toggle::Ortho);
    assert!(dirty.contains(Dirty::OVERLAY), "the rubber band must jump");
    assert_eq!(h.prompt(), prompt);
    h.click_world(p(120.0, 40.0));
    h.toggle(Toggle::Ortho);
    h.click_world(p(150.0, 90.0));
    h.enter("");
    assert_eq!(
        h.lines(),
        vec![
            line(0.0, 0.0, 50.0, 20.0),
            line(50.0, 20.0, 120.0, 20.0),
            line(120.0, 20.0, 150.0, 90.0)
        ]
    );
}

#[test]
fn ortho_constrains_a_move_displacement() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.toggle(Toggle::Osnap); // off: this test is about ORTHO alone
    h.toggle(Toggle::Ortho);
    h.click_world(p(5.0, 0.0));
    h.enter_all(&["M", "0,0"]);
    h.click_world(p(12.0, 60.0));
    assert_eq!(h.lines(), vec![line(0.0, 60.0, 10.0, 60.0)]);
}

#[test]
fn ortho_constrains_a_radius_given_with_the_pointer() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["C", "0,0"]);
    h.click_world(p(30.0, 40.0));
    let Entity::Circle(circle) = &h.entities()[0] else {
        panic!("expected a circle");
    };
    assert_eq!(
        circle.radius, 40.0,
        "the vertical leg, not the free distance of 50"
    );
}

#[test]
fn the_opposite_corner_of_a_rectangle_is_not_constrained() {
    let mut h = with(Toggle::Ortho);
    h.enter_all(&["REC", "0,0"]);
    h.move_to_world(p(80.0, 30.0));
    assert_eq!(h.session.aim().unwrap().track, None);
    h.click_world(p(80.0, 30.0));
    assert_eq!(
        h.entities(),
        vec![Entity::from(PolylineEntity::new(
            vec![p(0.0, 0.0), p(80.0, 0.0), p(80.0, 30.0), p(0.0, 30.0)],
            true
        ))]
    );
}

// ── POLAR (#83) ──

#[test]
fn polar_locks_near_a_tracked_direction_and_releases_away_from_it() {
    let mut h = with(Toggle::Polar);
    h.enter_all(&["L", "0,0"]);

    // Five pixels off the horizontal: locked.
    h.move_to_world(p(80.0, 5.0));
    let aim = h.session.aim().unwrap();
    assert_eq!(aim.point, p(80.0, 0.0));
    assert_eq!(aim.track.unwrap().kind, TrackKind::Polar);
    assert_eq!(tracking_line(&h), Some((p(0.0, 0.0), p(80.0, 0.0))));
    assert_eq!(
        h.session.ui_state().tracking.as_deref(),
        Some("Polar: 80.0000 < 0°")
    );

    // Thirty pixels off: released.
    h.move_to_world(p(80.0, 30.0));
    let aim = h.session.aim().unwrap();
    assert_eq!((aim.point, aim.track), (p(80.0, 30.0), None));
    assert_eq!(tracking_line(&h), None);
    assert_eq!(h.session.ui_state().tracking, None);

    h.click_world(p(80.0, 30.0));
    h.click_world(p(84.0, 130.0)); // near the vertical through (80, 30)
    h.enter("");
    assert_eq!(
        h.lines(),
        vec![line(0.0, 0.0, 80.0, 30.0), line(80.0, 30.0, 80.0, 130.0)]
    );
}

#[test]
fn the_polar_increment_is_configurable() {
    let mut h = with(Toggle::Polar);
    h.enter_all(&["POLARANG", "45", "L", "0,0"]);
    h.move_to_world(p(72.0, 69.0));
    let track = h.session.aim().unwrap().track.unwrap();
    assert_eq!(track.angle, 45.0);
    assert!(track.point.distance_to(p(70.5, 70.5)) < 1e-9);
}

/// Typed distance along a tracked angle.
#[test]
fn direct_distance_runs_along_the_tracked_angle() {
    let mut h = with(Toggle::Polar);
    h.enter_all(&["POLARANG", "30", "L", "0,0"]);
    // Near the 60° direction, not on it.
    h.move_to_world(p(52.0, 84.0));
    assert_eq!(h.session.aim().unwrap().track.unwrap().angle, 60.0);
    h.enter_all(&["100", ""]);
    let end = h.lines()[0].end;
    assert!(
        end.distance_to(p(50.0, 86.602_540_378_443_86)) < 1e-9,
        "{end:?}"
    );
}

#[test]
fn the_polar_aperture_is_in_pixels_not_world_units() {
    let mut h = with(Toggle::Polar);
    h.enter_all(&["L", "0,0"]);
    h.move_to_world(p(80.0, 5.0));
    assert!(h.session.aim().unwrap().track.is_some(), "5px off at 1:1");

    // Zoom in twenty times: the same offset in world units is now 100 pixels.
    let about = h.session.view().world_to_screen(p(0.0, 0.0));
    for _ in 0..3 {
        h.session.wheel(about, -500.0, Modifiers::NONE);
    }
    assert!(h.session.view().scale() > 20.0);
    h.move_to_world(p(8.0, 5.0));
    assert_eq!(h.session.aim().unwrap().track, None);
    h.move_to_world(p(8.0, 0.2));
    assert!(h.session.aim().unwrap().track.is_some());
}

#[test]
fn an_object_snap_beats_polar_and_typed_input_ignores_it() {
    let mut h = Harness::new();
    h.enter_all(&["L", "100,6", "150,60", ""]);
    h.toggle(Toggle::Polar);
    h.enter_all(&["L", "0,0"]);
    // Within reach of the horizontal *and* of the endpoint at (100, 6).
    h.move_to_world(p(99.0, 4.0));
    let aim = h.session.aim().unwrap();
    assert_eq!(aim.point, p(100.0, 6.0));
    assert!(aim.snap.is_some() && aim.track.is_none());

    h.enter_all(&["70,3", ""]);
    assert_eq!(h.lines()[1], line(0.0, 0.0, 70.0, 3.0));
}

/// F10 toggles mid-command.
#[test]
fn f10_takes_effect_mid_command_without_cancelling_it() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0"]);
    h.move_to_world(p(80.0, 5.0));
    assert_eq!(h.session.aim().unwrap().point, p(80.0, 5.0));

    h.toggle(Toggle::Polar);
    assert_eq!(h.prompt(), "Specify next point or [Undo]:");
    assert_eq!(h.session.aim().unwrap().point, p(80.0, 0.0));
    h.toggle(Toggle::Polar);
    assert_eq!(h.session.aim().unwrap().point, p(80.0, 5.0));
}

/// The defined interaction with ORTHO: they are never on together.
#[test]
fn ortho_and_polar_switch_each_other_off() {
    let mut h = with(Toggle::Polar);
    h.toggle(Toggle::Ortho);
    let toggles = h.session.toggles();
    assert!(toggles.is_on(Toggle::Ortho) && !toggles.is_on(Toggle::Polar));
    let log = h.log();
    assert_eq!(&log[log.len() - 2..], ["<ORTHO on>", "<POLAR off>"]);

    h.toggle(Toggle::Polar);
    let toggles = h.session.toggles();
    assert!(toggles.is_on(Toggle::Polar) && !toggles.is_on(Toggle::Ortho));

    // Switching one off does not switch the other on.
    h.toggle(Toggle::Polar);
    let toggles = h.session.toggles();
    assert!(!toggles.is_on(Toggle::Polar) && !toggles.is_on(Toggle::Ortho));
}

#[test]
fn drafting_aids_never_touch_the_document() {
    let mut h = with(Toggle::Polar);
    h.enter_all(&["L", "0,0"]);
    let revision = h.session.drawing().revision();
    for step in 0..40 {
        let at = ScreenPoint::new(400.0 + f64::from(step) * 7.0, 300.0 - f64::from(step));
        let dirty = h.session.pointer_move(at, Modifiers::NONE);
        assert!(!dirty.contains(Dirty::SCENE));
    }
    assert_eq!(h.session.drawing().revision(), revision);
}
