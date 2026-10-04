//! Object snaps as the user meets them: through pointer input to a command.

use cad_core::Point2;
use cad_document::{Entity, LineEntity};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem, SnapMode, SnapPoint, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
    LineEntity::new(p(x0, y0), p(x1, y1))
}

/// One horizontal line from the origin to (100, 0).
fn one_line() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h
}

fn marker(h: &Harness) -> Option<(SnapMode, Point2)> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::SnapMarker { mode, at } => Some((mode, at)),
        _ => None,
    })
}

/// #76: the committed point equals the exact snap point, not the cursor position.
#[test]
fn a_click_near_an_endpoint_commits_the_exact_endpoint() {
    let mut h = one_line();
    h.enter("L");
    h.click_world(p(97.5, 3.25));
    h.enter_all(&["@0,50", ""]);
    assert_eq!(h.lines()[1], line(100.0, 0.0, 100.0, 50.0));
}

#[test]
fn a_click_away_from_any_snap_point_commits_the_cursor_position() {
    let mut h = one_line();
    h.enter("L");
    h.click_world(p(70.0, 40.0));
    h.click_world(p(75.0, 3.0));
    h.enter("");
    assert_eq!(h.lines()[1], line(70.0, 40.0, 75.0, 3.0));
}

#[test]
fn typed_coordinates_are_never_snapped() {
    let mut h = one_line();
    h.move_to_world(p(99.0, 1.0));
    h.enter_all(&["L", "99,1", "50,50", ""]);
    assert_eq!(h.lines()[1], line(99.0, 1.0, 50.0, 50.0));
}

/// #80: the marker shown always corresponds to the point that would be committed.
#[test]
fn the_marker_the_status_bar_and_the_click_agree() {
    let mut h = one_line();
    h.enter("L");
    h.move_to_world(p(51.0, -2.0));

    assert_eq!(
        h.session.snap(),
        Some(SnapPoint {
            mode: SnapMode::Midpoint,
            point: p(50.0, 0.0)
        })
    );
    assert_eq!(marker(&h), Some((SnapMode::Midpoint, p(50.0, 0.0))));
    let ui = h.session.ui_state();
    assert_eq!(ui.snap, Some("Midpoint"));
    assert_eq!(ui.cursor, Some((50.0, 0.0)));

    h.click_world(p(51.0, -2.0));
    h.enter_all(&["@10,10", ""]);
    assert_eq!(h.lines()[1].start, p(50.0, 0.0));
}

#[test]
fn the_rubber_band_follows_the_snap_while_the_crosshair_follows_the_pointer() {
    let mut h = one_line();
    h.enter_all(&["L", "0,80"]);
    h.move_to_world(p(98.0, 2.0));

    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::PreviewSegment {
        start: p(0.0, 80.0),
        end: p(100.0, 0.0)
    }));
    assert!(overlay.contains(&OverlayItem::Crosshair { at: p(98.0, 2.0) }));
}

#[test]
fn there_is_no_snap_when_no_command_is_asking_for_a_point() {
    let mut h = one_line();
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(h.session.snap(), None, "idle: the pointer selects");
    assert_eq!(h.session.ui_state().snap, None);

    h.enter("M");
    assert_eq!(h.prompt(), "Select objects:");
    assert_eq!(h.session.snap(), None, "selecting objects");

    h.escape();
    h.enter("OSNAP");
    assert_eq!(h.session.snap(), None, "a keyword prompt");
}

#[test]
fn a_point_given_as_a_radius_is_snapped_too() {
    let mut h = one_line();
    h.enter_all(&["C", "0,0"]);
    h.click_world(p(49.0, 2.0));
    let Entity::Circle(circle) = &h.entities()[1] else {
        panic!("expected a circle");
    };
    assert_eq!(circle.radius, 50.0);
}

#[test]
fn direct_distance_aims_at_the_snapped_point() {
    let mut h = one_line();
    h.enter_all(&["L", "100,100"]);
    // Hovering just off the endpoint: the direction is exactly towards it.
    h.move_to_world(p(102.0, 3.0));
    h.enter_all(&["40", ""]);
    assert_eq!(h.lines()[1], line(100.0, 100.0, 100.0, 60.0));
}

// ── F3 and running snaps (#80) ──

#[test]
fn f3_switches_snapping_off_and_on_without_disturbing_the_command() {
    let mut h = one_line();
    h.enter("L");
    h.move_to_world(p(99.0, 1.0));
    assert!(h.session.snap().is_some());

    let dirty = h.toggle(Toggle::Osnap);
    assert!(dirty.contains(Dirty::OVERLAY), "the marker must disappear");
    assert_eq!(h.session.snap(), None);
    assert_eq!(marker(&h), None);
    h.click_world(p(99.0, 1.0));
    h.toggle(Toggle::Osnap);
    h.click_world(p(99.0, -1.0));
    h.enter("");
    assert_eq!(h.lines()[1], line(99.0, 1.0, 100.0, 0.0));
}

#[test]
fn only_the_running_modes_are_offered() {
    let mut h = one_line();
    h.enter_all(&["OSNAP", "E", ""]);
    h.enter("L");
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(h.session.snap(), None, "Endpoint is switched off");
    h.move_to_world(p(49.0, 1.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.mode),
        Some(SnapMode::Midpoint)
    );
}

#[test]
fn nearest_can_be_switched_on_and_lands_on_the_line() {
    let mut h = one_line();
    h.enter_all(&["OSNAP", "N", ""]);
    h.enter("L");
    h.click_world(p(80.0, 4.0));
    h.enter_all(&["@0,10", ""]);
    assert_eq!(h.lines()[1].start, p(80.0, 0.0));
}

// ── perpendicular and tangent (#117) through a command ──

fn with_constructions(h: &mut Harness) {
    h.enter_all(&["OSNAP", "P", "T", ""]);
}

#[test]
fn a_line_is_drawn_perpendicular_to_another() {
    let mut h = one_line();
    with_constructions(&mut h);
    h.enter_all(&["L", "30,70"]);

    // Hovering the line anywhere marks the foot of the perpendicular.
    h.move_to_world(p(75.0, 1.0));
    assert_eq!(marker(&h), Some((SnapMode::Perpendicular, p(30.0, 0.0))));
    assert_eq!(h.session.ui_state().snap, Some("Perpendicular"));
    h.click_world(p(75.0, 1.0));
    h.enter("");
    assert_eq!(h.lines()[1], line(30.0, 70.0, 30.0, 0.0));
}

#[test]
fn a_line_is_drawn_tangent_to_a_circle() {
    let mut h = Harness::new();
    h.enter_all(&["C", "0,0", "30"]);
    with_constructions(&mut h);
    h.enter_all(&["L", "50,0"]);
    // On the upper half of the circle, nearer to the upper tangent point
    // than to anything else that is offered.
    h.click_world(p(10.26, 28.19));
    h.enter("");

    let tangent = h.lines()[0];
    assert!(tangent.end.distance_to(p(18.0, 24.0)) < 1e-9, "{tangent:?}");
    // Tangent: the radius to the touching point is perpendicular to the line.
    let along = tangent.end - tangent.start;
    assert!((tangent.end.x * along.x + tangent.end.y * along.y).abs() < 1e-9);
}

#[test]
fn constructions_are_not_offered_for_the_first_point_of_a_command() {
    let mut h = one_line();
    with_constructions(&mut h);
    h.enter("L");
    // No point to construct from yet: only the ordinary snaps apply.
    h.move_to_world(p(75.0, 1.0));
    assert_eq!(h.session.snap(), None);
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.mode),
        Some(SnapMode::Endpoint)
    );
}

#[test]
fn a_grip_can_be_dropped_perpendicular_to_another_object() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", "", "L", "20,80", "60,40", ""]);
    with_constructions(&mut h);
    h.click_world(p(40.0, 60.0)); // select the second line, away from its grips
    h.click_world(p(60.0, 40.0)); // take its lower endpoint
    h.click_world(p(90.0, 1.0)); // hover the first line: perpendicular from where the grip was
    assert_eq!(h.lines()[1], line(20.0, 80.0, 60.0, 0.0));
}

// ── intersection (#78) through a command ──

#[test]
fn a_click_near_a_crossing_commits_the_exact_intersection() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,100", "", "L", "0,100", "100,0", ""]);
    h.enter("C");
    h.click_world(p(52.0, 49.0));
    h.enter("10");
    let Entity::Circle(circle) = &h.entities()[2] else {
        panic!("expected a circle");
    };
    assert_eq!(circle.center, p(50.0, 50.0));
}

// ── transient-edit exclusion (#79, DESIGN §18.2 LOCKED) ──

/// While moving a line, none of its own snap points are offered.
#[test]
fn a_line_being_moved_does_not_snap_to_itself() {
    let mut h = one_line();
    h.click_world(p(30.0, 0.0));
    h.enter_all(&["M", "0,0"]);
    assert_eq!(h.prompt(), "Specify second point:");

    for own in [p(99.0, 1.0), p(51.0, 1.0), p(1.0, 1.0)] {
        h.move_to_world(own);
        assert_eq!(h.session.snap(), None, "own snap point near {own:?}");
    }
    // The click therefore lands where the cursor is, not on the old endpoint.
    h.click_world(p(99.0, 1.0));
    assert_eq!(h.lines(), vec![line(99.0, 1.0, 199.0, 1.0)]);
}

/// Other entities remain snappable during the same edit.
#[test]
fn other_entities_stay_snappable_while_one_is_moved() {
    let mut h = one_line();
    h.enter_all(&["L", "200,50", "200,150", ""]);
    h.click_world(p(30.0, 0.0));
    h.enter_all(&["M", "0,0"]);

    h.move_to_world(p(201.0, 51.0));
    assert_eq!(
        h.session.snap(),
        Some(SnapPoint {
            mode: SnapMode::Endpoint,
            point: p(200.0, 50.0)
        })
    );
    h.click_world(p(201.0, 51.0));
    assert_eq!(h.lines()[0], line(200.0, 50.0, 300.0, 50.0));
}

#[test]
fn the_base_point_of_a_move_can_snap_to_the_entity_about_to_move() {
    let mut h = one_line();
    h.click_world(p(30.0, 0.0));
    h.enter("M");
    assert_eq!(h.prompt(), "Specify base point:");
    // Nothing is in motion yet, so the line's own endpoint is a fair target.
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(h.session.snap().map(|snap| snap.point), Some(p(100.0, 0.0)));
}

#[test]
fn copies_keep_their_sources_in_place_so_the_sources_stay_snappable() {
    let mut h = one_line();
    h.click_world(p(30.0, 0.0));
    h.enter_all(&["CO", "0,0"]);
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(h.session.snap().map(|snap| snap.point), Some(p(100.0, 0.0)));
}

// ── snapping never touches the document ──

#[test]
fn hovering_over_snap_points_changes_neither_the_drawing_nor_the_scene() {
    let mut h = one_line();
    h.enter("L");
    let revision = h.session.drawing().revision();
    for x in [0.0, 25.0, 50.0, 75.0, 100.0] {
        let dirty = h.move_to_world(p(x, 2.0));
        assert!(!dirty.contains(Dirty::SCENE));
    }
    assert_eq!(h.session.drawing().revision(), revision);
}
