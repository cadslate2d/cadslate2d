//! Grips and grip editing, through the same session API the browser uses.

use std::f64::consts::PI;

use cad_core::{EntityId, Point2, ScreenPoint};
use cad_document::{ArcEntity, CircleEntity, Entity, GripKind, LineEntity, PolylineEntity};

use crate::testing::Harness;
use crate::{Dirty, GripId, Modifiers, OverlayItem, PointerButton, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// A horizontal line from the origin to (100, 0), selected by a click that
/// is not on any of its grips.
fn selected_line() -> (Harness, EntityId) {
    let mut h = Harness::with_entities([line(0.0, 0.0, 100.0, 0.0)]);
    h.click_world(p(30.0, 0.0));
    let id = h.ids()[0];
    (h, id)
}

fn grip_points(h: &Harness) -> Vec<Point2> {
    h.session.grips().iter().map(|grip| grip.position).collect()
}

fn hot_grip(h: &Harness) -> Option<Point2> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::HotGrip { at } => Some(at),
        _ => None,
    })
}

// ── model and display (#85) ──

/// Grips appear on selection and disappear on deselection with no committed upload.
#[test]
fn grips_appear_with_the_selection_and_go_with_it() {
    let mut h = Harness::with_entities([line(0.0, 0.0, 100.0, 0.0)]);
    let revision = h.session.drawing().revision();
    assert!(h.session.grips().is_empty());

    let dirty = h.click_world(p(30.0, 0.0));
    assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
    assert_eq!(
        grip_points(&h),
        vec![p(0.0, 0.0), p(50.0, 0.0), p(100.0, 0.0)]
    );
    let kinds: Vec<GripKind> = h.session.grips().iter().map(|grip| grip.kind).collect();
    assert_eq!(
        kinds,
        vec![GripKind::Endpoint, GripKind::Midpoint, GripKind::Endpoint]
    );
    assert!(
        h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::Grips { at } if at.len() == 3))
    );

    let dirty = h.escape();
    assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
    assert!(h.session.grips().is_empty());
    assert_eq!(h.session.drawing().revision(), revision);
}

/// Grip positions track undo and redo.
#[test]
fn grips_follow_the_entity_through_undo_and_redo() {
    let (mut h, _) = selected_line();
    h.enter_all(&["M", "0,0", "0,50"]);
    h.click_world(p(30.0, 50.0));
    assert_eq!(
        grip_points(&h),
        vec![p(0.0, 50.0), p(50.0, 50.0), p(100.0, 50.0)]
    );

    h.undo();
    assert_eq!(
        grip_points(&h),
        vec![p(0.0, 0.0), p(50.0, 0.0), p(100.0, 0.0)]
    );
    h.redo();
    assert_eq!(
        grip_points(&h),
        vec![p(0.0, 50.0), p(50.0, 50.0), p(100.0, 50.0)]
    );
}

#[test]
fn the_grip_under_the_cursor_is_hot() {
    let (mut h, id) = selected_line();
    assert_eq!(hot_grip(&h), None);

    h.move_to_world(p(97.0, 3.0));
    assert_eq!(
        h.session.hovered_grip().map(|grip| grip.id),
        Some(GripId {
            entity: id,
            index: 2
        })
    );
    assert_eq!(hot_grip(&h), Some(p(100.0, 0.0)));

    h.move_to_world(p(80.0, 3.0));
    assert_eq!(h.session.hovered_grip(), None);
    assert_eq!(hot_grip(&h), None);
}

/// Grip hit testing takes priority over entity picking.
#[test]
fn a_click_on_a_grip_takes_the_grip_not_the_entity_behind_it() {
    // A second line passes right through the first one's endpoint.
    let mut h =
        Harness::with_entities([line(0.0, 0.0, 100.0, 0.0), line(100.0, -50.0, 100.0, 50.0)]);
    h.click_world(p(30.0, 0.0));
    assert_eq!(h.session.selection().len(), 1);

    h.click_world(p(100.0, 0.0));
    assert_eq!(h.session.ui_state().command, Some("STRETCH"));
    assert_eq!(
        h.session.selection().len(),
        1,
        "the crossing line was not picked"
    );

    // With Shift held, a click is a selection click even on a grip.
    h.escape();
    let at = h.session.view().world_to_screen(p(50.0, 0.0));
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    h.session.pointer_down(at, PointerButton::Primary, shift);
    assert_eq!(h.session.ui_state().command, None);
    assert!(
        h.session.selection().is_empty(),
        "Shift+click removed the line"
    );
}

#[test]
fn grips_are_hidden_while_another_command_runs() {
    let (mut h, _) = selected_line();
    h.enter("L");
    assert!(h.session.grips().is_empty());
    // A click where a grip was is a point for the command.
    h.toggle(Toggle::Osnap);
    h.click_world(p(100.0, 0.0));
    assert_eq!(h.session.ui_state().command, Some("LINE"));
    h.escape();
    assert_eq!(h.session.grips().len(), 3, "back at the idle prompt");
}

// ── the grip edit (#86) ──

#[test]
fn clicking_a_grip_starts_a_stretch() {
    let (mut h, id) = selected_line();
    h.click_world(p(100.0, 0.0));
    assert_eq!(h.prompt(), "Specify stretch point:");
    assert_eq!(h.log().last().unwrap(), "** STRETCH **");
    assert_eq!(
        h.session.active_grip().map(|grip| grip.id),
        Some(GripId {
            entity: id,
            index: 2
        })
    );
    // The grips stay on screen, with the one in hand marked where it started.
    assert_eq!(h.session.grips().len(), 3);
    assert_eq!(hot_grip(&h), Some(p(100.0, 0.0)));
}

/// Pointer movement during a grip edit does not change `Drawing::revision`.
#[test]
fn a_grip_drag_is_previewed_without_touching_the_document() {
    let (mut h, id) = selected_line();
    h.click_world(p(100.0, 0.0));
    let revision = h.session.drawing().revision();

    for step in 0..100 {
        let dirty = h.move_to_world(p(100.0 + f64::from(step), f64::from(step) * 2.0));
        assert!(dirty.contains(Dirty::OVERLAY) && !dirty.contains(Dirty::SCENE));
    }
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);

    h.move_to_world(p(150.0, 80.0));
    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::PreviewGrip {
        id: GripId {
            entity: id,
            index: 2
        },
        to: p(150.0, 80.0)
    }));
    assert!(overlay.contains(&OverlayItem::PreviewSegment {
        start: p(100.0, 0.0),
        end: p(150.0, 80.0)
    }));
}

/// One undo reverts one grip edit and keeps the entity selected.
#[test]
fn a_click_commits_one_transaction_and_the_entity_stays_selected() {
    let (mut h, id) = selected_line();
    h.click_world(p(100.0, 0.0));
    let dirty = h.click_world(p(150.0, 80.0));

    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 150.0, 80.0)]);
    assert_eq!(h.ids(), vec![id]);
    assert!(h.session.selection().contains(id));
    assert_eq!(
        grip_points(&h),
        vec![p(0.0, 0.0), p(75.0, 40.0), p(150.0, 80.0)]
    );

    h.undo();
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);
    assert!(
        h.session.selection().contains(id),
        "still selected after undo"
    );
    assert!(!h.session.ui_state().can_undo, "it was one step");
    h.redo();
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 150.0, 80.0)]);
}

#[test]
fn enter_drops_the_grip_at_the_pointer() {
    let (mut h, _) = selected_line();
    h.click_world(p(0.0, 0.0));
    h.move_to_world(p(-40.0, 30.0));
    h.enter("");
    assert_eq!(h.entities(), vec![line(-40.0, 30.0, 100.0, 0.0)]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn enter_with_the_pointer_off_the_canvas_changes_nothing() {
    let (mut h, id) = selected_line();
    h.click_world(p(0.0, 0.0));
    h.session.pointer_leave();
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);
    assert!(h.session.selection().contains(id));
    assert!(!h.session.ui_state().can_undo);
}

#[test]
fn escape_cancels_the_edit_and_keeps_the_selection() {
    let (mut h, id) = selected_line();
    let revision = h.session.drawing().revision();
    h.click_world(p(100.0, 0.0));
    h.move_to_world(p(150.0, 80.0));
    h.escape();

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(h.session.selection().contains(id));
    assert_eq!(h.session.grips().len(), 3);
}

#[test]
fn a_grip_can_be_dragged_and_dropped_as_well_as_clicked() {
    let (mut h, _) = selected_line();
    let press = h.session.view().world_to_screen(p(100.0, 0.0));
    let release = h.session.view().world_to_screen(p(120.0, 60.0));
    h.session
        .pointer_down(press, PointerButton::Primary, Modifiers::NONE);
    h.session.pointer_move(release, Modifiers::NONE);
    let dirty = h
        .session
        .pointer_up(release, PointerButton::Primary, Modifiers::NONE);

    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 120.0, 60.0)]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn releasing_where_the_grip_was_pressed_keeps_it_in_hand() {
    let (mut h, _) = selected_line();
    let press = h.session.view().world_to_screen(p(100.0, 0.0));
    h.session
        .pointer_down(press, PointerButton::Primary, Modifiers::NONE);
    h.session.pointer_up(
        ScreenPoint::new(press.x + 1.0, press.y),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(h.prompt(), "Specify stretch point:");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);
}

#[test]
fn dropping_a_grip_where_it_was_is_not_an_edit() {
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    h.enter("100,0");
    assert_eq!(h.prompt(), "Command:");
    assert!(!h.session.ui_state().can_undo);
    assert!(!h.session.ui_state().unsaved_changes);
}

// ── per entity (#87–#90) ──

#[test]
fn line_grips_stretch_an_end_or_move_the_whole_line() {
    let (mut h, _) = selected_line();
    h.click_world(p(0.0, 0.0));
    h.enter("-20,-20");
    assert_eq!(h.entities(), vec![line(-20.0, -20.0, 100.0, 0.0)]);

    // The midpoint grip, now at (40, -10), moves the line.
    h.click_world(p(40.0, -10.0));
    h.enter("@0,110");
    assert_eq!(h.entities(), vec![line(-20.0, 90.0, 100.0, 110.0)]);
}

#[test]
fn a_line_cannot_be_stretched_onto_its_own_other_end() {
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    h.enter("0,0");
    assert_eq!(
        h.log().last().unwrap(),
        "The object cannot be stretched to that point."
    );
    assert_eq!(h.prompt(), "Specify stretch point:", "still in hand");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 0.0)]);
}

fn selected_circle() -> Harness {
    let mut h = Harness::with_entities([Entity::from(CircleEntity::new(p(0.0, 0.0), 50.0))]);
    h.click_world(p(50.0 * 0.6, 50.0 * 0.8)); // on the circle, away from every grip
    assert_eq!(h.session.grips().len(), 5);
    h
}

#[test]
fn circle_grips_move_the_circle_or_set_its_radius() {
    let mut h = selected_circle();
    h.click_world(p(0.0, 0.0)); // centre
    h.enter("10,20");
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(10.0, 20.0), 50.0).into()]
    );

    h.click_world(p(10.0, 70.0)); // the north quadrant
    h.enter("10,100");
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(10.0, 20.0), 80.0).into()]
    );

    for quadrant in [p(90.0, 20.0), p(-70.0, 20.0), p(10.0, -60.0)] {
        h.click_world(quadrant);
        h.enter("40,60"); // 30 across, 40 up from the centre
        assert_eq!(
            h.entities(),
            vec![CircleEntity::new(p(10.0, 20.0), 50.0).into()]
        );
        h.undo();
    }
}

/// #88: dragging a quadrant grip to the centre is rejected.
#[test]
fn a_quadrant_grip_dropped_on_the_centre_is_refused() {
    let mut h = selected_circle();
    h.toggle(Toggle::Osnap);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "The object cannot be stretched to that point."
    );
    assert_eq!(h.prompt(), "Specify stretch point:");
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(0.0, 0.0), 50.0).into()]
    );
    assert!(!h.session.ui_state().can_undo);
}

/// The upper half circle of radius 50: (50,0) over (0,50) to (-50,0).
fn selected_arc() -> Harness {
    let mut h = Harness::with_entities([Entity::from(ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI))]);
    h.click_world(p(30.0, 40.0));
    assert_eq!(h.session.grips().len(), 4);
    h
}

fn arc_of(h: &Harness) -> cad_geometry::Arc {
    match &h.entities()[0] {
        Entity::Arc(arc) => arc.arc(),
        other => panic!("expected an arc, got {other:?}"),
    }
}

#[test]
fn the_arc_midpoint_grip_reshapes_the_arc_between_its_fixed_ends() {
    let mut h = selected_arc();
    h.click_world(p(0.0, 50.0));
    h.enter("0,20");
    let arc = arc_of(&h);
    assert!(arc.start_point().distance_to(p(50.0, 0.0)) < 1e-9);
    assert!(arc.end_point().distance_to(p(-50.0, 0.0)) < 1e-9);
    assert!(arc.mid_point().distance_to(p(0.0, 20.0)) < 1e-9);
}

#[test]
fn an_arc_endpoint_grip_keeps_the_midpoint_and_the_other_end() {
    let mut h = selected_arc();
    h.click_world(p(50.0, 0.0));
    h.enter("50,50");
    let arc = arc_of(&h);
    assert!(arc.start_point().distance_to(p(50.0, 50.0)) < 1e-9);
    assert!(arc.end_point().distance_to(p(-50.0, 0.0)) < 1e-9);
    assert!(
        arc.distance_to_point(p(0.0, 50.0)) < 1e-9,
        "still through the old midpoint"
    );
}

#[test]
fn the_arc_centre_grip_moves_the_arc() {
    let mut h = selected_arc();
    h.click_world(p(0.0, 0.0));
    h.enter("@100,0");
    assert_eq!(
        h.entities(),
        vec![ArcEntity::new(p(100.0, 0.0), 50.0, 0.0, PI).into()]
    );
}

/// #89: degenerate (collinear) positions.
#[test]
fn an_arc_grip_dropped_in_line_with_the_other_two_points_is_refused() {
    let mut h = selected_arc();
    h.click_world(p(0.0, 50.0));
    h.enter("10,0"); // on the chord between the two ends
    assert_eq!(
        h.log().last().unwrap(),
        "The object cannot be stretched to that point."
    );
    assert_eq!(h.prompt(), "Specify stretch point:");
    h.enter("0,-50"); // the other side is fine
    assert!(arc_of(&h).mid_point().distance_to(p(0.0, -50.0)) < 1e-9);
}

fn square(closed: bool) -> PolylineEntity {
    PolylineEntity::new(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
        closed,
    )
}

#[test]
fn polyline_vertex_grips_move_one_vertex_open_or_closed() {
    for closed in [false, true] {
        let mut h = Harness::with_entities([Entity::from(square(closed))]);
        h.click_world(p(50.0, 0.0));
        assert_eq!(h.session.grips().len(), 4);

        h.click_world(p(100.0, 100.0));
        h.enter("150,160");
        let mut expected = square(closed);
        expected.vertices[2] = p(150.0, 160.0);
        assert_eq!(
            h.entities(),
            vec![expected.clone().into()],
            "closed: {closed}"
        );

        // The first vertex, whose neighbours differ between open and closed.
        h.click_world(p(0.0, 0.0));
        h.enter("-30,-30");
        expected.vertices[0] = p(-30.0, -30.0);
        assert_eq!(h.entities(), vec![expected.into()]);
    }
}

/// #90: a 10k-vertex polyline — one grip drag, one transaction.
#[test]
fn dragging_a_vertex_of_a_ten_thousand_vertex_polyline_commits_one_transaction() {
    let vertices: Vec<Point2> = (0..10_000)
        .map(|index| {
            p(
                f64::from(index) * 0.05 - 250.0,
                if index % 2 == 0 { 0.0 } else { 5.0 },
            )
        })
        .collect();
    let mut h =
        Harness::with_entities([Entity::from(PolylineEntity::new(vertices.clone(), false))]);
    h.click_world(p(-250.0, 0.0));
    assert_eq!(h.session.grips().len(), 10_000);

    // The last vertex, at (249.95, 5).
    h.click_world(p(249.95, 5.0));
    assert_eq!(
        h.session.active_grip().map(|grip| grip.id.index),
        Some(9_999)
    );
    let revision = h.session.drawing().revision();
    for step in 0..50 {
        h.move_to_world(p(250.0 + f64::from(step), 50.0));
    }
    assert_eq!(h.session.drawing().revision(), revision);
    h.click_world(p(300.0, 50.0));

    let Entity::Polyline(edited) = &h.entities()[0] else {
        panic!("a polyline stays a polyline");
    };
    assert_eq!(edited.vertices.len(), 10_000);
    assert_eq!(edited.vertices[9_999], p(300.0, 50.0));
    assert_eq!(edited.vertices[..9_999], vertices[..9_999]);

    h.undo();
    assert!(
        !h.session.ui_state().can_undo,
        "one transaction, one undo step"
    );
    let Entity::Polyline(restored) = &h.entities()[0] else {
        panic!("a polyline stays a polyline");
    };
    assert_eq!(restored.vertices, vertices);
}

// ── precise placement (#91) ──

#[test]
fn a_grip_can_be_placed_by_typed_coordinates_of_every_kind() {
    // Absolute.
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    h.enter("70,70");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 70.0, 70.0)]);

    // Relative: measured from where the grip was.
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    h.enter("@10,-5");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 110.0, -5.0)]);

    // Relative polar.
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    h.enter("@50<90");
    assert!(h.lines()[0].end.distance_to(p(100.0, 50.0)) < 1e-9);

    // Direct distance: towards the pointer, from where the grip was.
    let (mut h, _) = selected_line();
    h.toggle(Toggle::Osnap);
    h.click_world(p(100.0, 0.0));
    h.move_to_world(p(100.0, -30.0));
    h.enter("75");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, -75.0)]);
}

#[test]
fn a_grip_snaps_to_other_entities() {
    let mut h =
        Harness::with_entities([line(0.0, 0.0, 100.0, 0.0), line(200.0, 50.0, 200.0, 150.0)]);
    h.click_world(p(30.0, 0.0));
    h.click_world(p(100.0, 0.0));

    h.move_to_world(p(198.0, 52.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.point),
        Some(p(200.0, 50.0))
    );
    h.click_world(p(198.0, 52.0));
    assert_eq!(h.entities()[0], line(0.0, 0.0, 200.0, 50.0));
}

/// The edited entity's own stale geometry is never a snap target.
#[test]
fn a_grip_never_snaps_to_the_entity_it_belongs_to() {
    let (mut h, _) = selected_line();
    h.click_world(p(100.0, 0.0));
    for own in [p(1.0, 1.0), p(49.0, 1.0), p(99.0, 2.0)] {
        h.move_to_world(own);
        assert_eq!(h.session.snap(), None, "near its own {own:?}");
    }
    // So the grip can be dropped a hair away from where it was.
    h.click_world(p(99.0, 2.0));
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 99.0, 2.0)]);
}

#[test]
fn ortho_and_polar_constrain_a_grip_from_where_it_started() {
    let (mut h, _) = selected_line();
    h.toggle(Toggle::Ortho);
    h.click_world(p(100.0, 0.0));
    h.click_world(p(110.0, 70.0));
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 100.0, 70.0)]);

    let (mut h, _) = selected_line();
    h.toggle(Toggle::Polar);
    h.click_world(p(100.0, 0.0));
    h.move_to_world(p(160.0, 4.0)); // within reach of the horizontal through the grip
    assert!(h.session.aim().unwrap().track.is_some());
    h.click_world(p(160.0, 4.0));
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 160.0, 0.0)]);
}
