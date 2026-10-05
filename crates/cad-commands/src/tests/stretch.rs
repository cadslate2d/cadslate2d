//! STRETCH (#128) through the session. What a window does to each kind of
//! entity is tested in `cad-document/src/editing.rs`; these tests cover the
//! command: the window, what it takes hold of, previews, the transaction,
//! and refusals.

use std::f64::consts::PI;

use cad_core::{Aabb, Point2, Vec2};
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// A 100 × 60 box drawn as four separate lines: bottom, right, top, left.
fn box_of_lines() -> Harness {
    Harness::with_entities([
        line(0.0, 0.0, 100.0, 0.0),
        line(100.0, 0.0, 100.0, 60.0),
        line(100.0, 60.0, 0.0, 60.0),
        line(0.0, 60.0, 0.0, 0.0),
    ])
}

/// Starts STRETCH with a window over the right-hand side of the box.
fn window_over_the_right(h: &mut Harness) {
    h.enter_all(&["S", "80,-20", "130,80"]);
}

#[test]
fn ends_inside_the_window_move_and_the_rest_stays() {
    let mut h = box_of_lines();
    let ids = h.ids();
    h.enter("S");
    assert_eq!(h.prompt(), "Specify first corner of crossing window:");
    h.enter("80,-20");
    assert_eq!(h.prompt(), "Specify opposite corner:");
    h.enter("130,80");
    assert_eq!(h.log().last().unwrap(), "3 found");
    assert_eq!(h.session.selection().len(), 3, "shown as a selection");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("100,0");
    assert_eq!(h.prompt(), "Specify second point:");
    h.enter("@40,0");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.entities(),
        vec![
            line(0.0, 0.0, 140.0, 0.0),    // stretched
            line(140.0, 0.0, 140.0, 60.0), // moved whole
            line(140.0, 60.0, 0.0, 60.0),  // stretched
            line(0.0, 60.0, 0.0, 0.0),     // untouched
        ]
    );
    assert_eq!(h.ids(), ids, "every object keeps its identity");
    assert!(h.session.selection().is_empty());

    h.undo();
    assert_eq!(h.entities()[0], line(0.0, 0.0, 100.0, 0.0), "one step");
    assert_eq!(h.entities()[1], line(100.0, 0.0, 100.0, 60.0));
    h.redo();
    assert_eq!(h.entities()[1], line(140.0, 0.0, 140.0, 60.0));
}

/// The window works whichever corners are given, in either order.
#[test]
fn the_window_is_a_crossing_window_however_it_is_drawn() {
    for corners in [
        ["130,80", "80,-20"],
        ["80,80", "130,-20"],
        ["130,-20", "80,80"],
    ] {
        let mut h = box_of_lines();
        h.enter_all(&["STRETCH", corners[0], corners[1], "0,0", "0,25"]);
        assert_eq!(
            h.entities()[1],
            line(100.0, 25.0, 100.0, 85.0),
            "{corners:?}"
        );
        assert_eq!(h.entities()[0], line(0.0, 0.0, 100.0, 25.0));
    }
}

/// A polyline with some vertices inside: those move, the bulges stay.
#[test]
fn a_polyline_with_some_vertices_inside_changes_shape() {
    let slot = PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 60.0), p(0.0, 60.0)],
        vec![0.0, 1.0, 0.0, 0.0],
        true,
    );
    let mut h = Harness::with_entities([slot.into()]);
    window_over_the_right(&mut h);
    assert_eq!(h.log().last().unwrap(), "1 found");
    h.enter_all(&["0,0", "30,10"]);
    assert_eq!(
        h.entities(),
        vec![
            PolylineEntity::with_bulges(
                vec![p(0.0, 0.0), p(130.0, 10.0), p(130.0, 70.0), p(0.0, 60.0)],
                vec![0.0, 1.0, 0.0, 0.0],
                true,
            )
            .into()
        ]
    );
}

/// Circles move only if their centre is inside.
#[test]
fn a_circle_moves_only_if_its_centre_is_inside() {
    let inside: Entity = CircleEntity::new(p(100.0, 30.0), 15.0).into();
    let reaching_in: Entity = CircleEntity::new(p(60.0, 30.0), 30.0).into();
    let mut h = Harness::with_entities([inside, reaching_in.clone()]);
    window_over_the_right(&mut h);
    assert_eq!(h.log().last().unwrap(), "1 found");
    h.enter_all(&["0,0", "50,0"]);
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(150.0, 30.0), 15.0).into(), reaching_in]
    );
}

#[test]
fn a_window_that_holds_nothing_says_so_and_asks_again() {
    // The circle is crossed by the window, the line runs right through it:
    // neither has a defining point inside.
    let mut h = Harness::with_entities([
        CircleEntity::new(p(60.0, 30.0), 30.0).into(),
        line(0.0, 10.0, 300.0, 10.0),
    ]);
    let revision = h.session.drawing().revision();
    window_over_the_right(&mut h);
    assert_eq!(h.log().last().unwrap(), "Nothing in the window to stretch.");
    assert_eq!(h.prompt(), "Specify first corner of crossing window:");
    assert!(h.session.selection().is_empty());
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);
}

/// An arc held by one end is re-fitted the way its end grip would do it.
#[test]
fn an_arc_follows_the_grip_rule() {
    let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
    let mut h = Harness::with_entities([arc.clone()]);
    h.enter_all(&["S", "40,-10", "60,10", "50,0", "70,-10"]);
    assert_eq!(
        h.entities(),
        vec![arc.grip_moved(0, p(70.0, -10.0)).unwrap()]
    );

    // Both ends inside: it moves as it is.
    let mut h = Harness::with_entities([arc.clone()]);
    h.enter_all(&["S", "-60,-10", "60,10", "0,0", "5,5"]);
    assert_eq!(h.entities(), vec![arc.translated(Vec2::new(5.0, 5.0))]);
}

#[test]
fn the_preview_shows_the_result_and_leaves_the_document_alone() {
    let mut h = box_of_lines();
    let ids = h.ids();
    let revision = h.session.drawing().revision();
    h.enter_all(&["S", "80,-20"]);
    h.move_to_world(p(130.0, 80.0));
    assert!(
        h.session.overlay().contains(&OverlayItem::SelectionBox {
            from: p(80.0, -20.0),
            to: p(130.0, 80.0),
            crossing: true
        }),
        "the window is drawn while it is being placed"
    );
    h.enter_all(&["130,80", "100,0"]);
    h.move_to_world(p(120.0, 0.0));

    let overlay = h.session.overlay();
    let reshaped: Vec<&Entity> = overlay
        .iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewEntity(entity) => Some(entity),
            _ => None,
        })
        .collect();
    assert_eq!(
        reshaped,
        vec![&line(0.0, 0.0, 120.0, 0.0), &line(120.0, 60.0, 0.0, 60.0)]
    );
    assert!(
        overlay.iter().any(|item| matches!(
            item,
            OverlayItem::PreviewTransformed { ids: moved, .. } if moved == &vec![ids[1]]
        )),
        "the line held whole is shown moved, without being copied"
    );
    assert_eq!(h.session.drawing().revision(), revision);

    // The click commits exactly what was shown.
    h.click_world(p(120.0, 0.0));
    assert_eq!(h.entities()[0], line(0.0, 0.0, 120.0, 0.0));
    assert_eq!(h.entities()[2], line(120.0, 60.0, 0.0, 60.0));
    assert_eq!(h.entities()[1], line(120.0, 0.0, 120.0, 60.0));
}

/// The window's corners are places on the screen: object snap leaves them
/// alone, and comes back for the base point.
#[test]
fn the_corners_of_the_window_are_not_snapped() {
    let mut h = box_of_lines();
    h.enter("S");
    h.move_to_world(p(101.0, 1.0)); // a pixel from the corner at (100, 0)
    assert!(h.session.snap().is_none());
    h.click_world(p(101.0, 1.0));
    h.move_to_world(p(-1.0, 61.0));
    assert!(h.session.snap().is_none());
    h.click_world(p(-1.0, 61.0));
    assert_eq!(h.prompt(), "Specify base point:");

    // The window is exactly where the clicks were: it holds the two top
    // corners and not (100, 0), a pixel outside it. Snapped onto that
    // corner, it would have taken the bottom line in as well.
    let window = Aabb::from_points(p(101.0, 1.0), p(-1.0, 61.0));
    assert!(!window.contains_point(p(100.0, 0.0)));
    assert_eq!(h.log().last().unwrap(), "3 found", "top, right, and left");

    h.move_to_world(p(101.0, 59.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.point),
        Some(p(100.0, 60.0)),
        "the base point snaps as usual"
    );
}

/// DESIGN §18.2: what is being stretched is not a snap target meanwhile.
#[test]
fn held_objects_are_not_snap_targets_while_they_are_stretched() {
    let mut h = box_of_lines();
    window_over_the_right(&mut h);
    h.enter("100,0");
    // (100, 30) is the midpoint of the right-hand line — which is held.
    h.move_to_world(p(101.0, 30.0));
    assert!(h.session.snap().is_none());
    // The left-hand line is not held: its midpoint is offered.
    h.move_to_world(p(1.0, 30.0));
    assert_eq!(h.session.snap().map(|snap| snap.point), Some(p(0.0, 30.0)));
}

#[test]
fn a_typed_distance_goes_towards_the_pointer() {
    let mut h = box_of_lines();
    window_over_the_right(&mut h);
    h.enter("100,0");
    h.move_to_world(p(300.0, 0.0));
    h.enter("25");
    assert_eq!(h.entities()[1], line(125.0, 0.0, 125.0, 60.0));
}

#[test]
fn an_object_that_would_collapse_is_left_and_the_rest_is_stretched() {
    // A stub from x = 70 whose right end the window holds, and a long line.
    let stub = line(70.0, 30.0, 100.0, 30.0);
    let mut h = Harness::with_entities([stub.clone(), line(0.0, 0.0, 100.0, 0.0)]);
    window_over_the_right(&mut h);
    h.enter_all(&["0,0", "-30,0"]); // the stub's right end would land on its left
    assert_eq!(
        h.log().last().unwrap(),
        "1 object could not be stretched that way and was left unchanged."
    );
    assert_eq!(h.entities(), vec![stub, line(0.0, 0.0, 70.0, 0.0)]);
    h.undo();
    assert_eq!(h.entities()[1], line(0.0, 0.0, 100.0, 0.0));
}

#[test]
fn cancelling_or_a_zero_move_changes_nothing() {
    let mut h = box_of_lines();
    let before = h.entities();
    let revision = h.session.drawing().revision();

    window_over_the_right(&mut h);
    h.enter("100,0");
    h.escape();
    assert_eq!(h.prompt(), "Command:");

    window_over_the_right(&mut h);
    h.enter_all(&["100,0", "100,0"]);
    assert_eq!(h.prompt(), "Command:");

    assert_eq!(h.entities(), before);
    assert_eq!(h.session.drawing().revision(), revision);
}
