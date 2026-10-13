//! ROTATE, SCALE, and MIRROR, through the same session API the browser uses.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::{EntityId, Point2};
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};
use cad_geometry::Transform;

use crate::testing::Harness;
use crate::{Dirty, OverlayItem, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line() -> Entity {
    LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into()
}

fn circle() -> Entity {
    CircleEntity::new(p(50.0, 50.0), 20.0).into()
}

/// A quarter arc from (140, 0) counter-clockwise to (100, 40).
fn arc() -> Entity {
    ArcEntity::new(p(100.0, 0.0), 40.0, 0.0, FRAC_PI_2).into()
}

fn polyline() -> Entity {
    PolylineEntity::new(vec![p(0.0, 60.0), p(40.0, 60.0), p(40.0, 100.0)], false).into()
}

/// One entity of every kind, all selected, at the idle prompt.
fn everything_selected() -> (Harness, Vec<EntityId>) {
    let mut h = Harness::with_entities([line(), circle(), arc(), polyline()]);
    h.select_box(p(-50.0, -50.0), p(200.0, 150.0));
    let ids = h.ids();
    assert_eq!(h.session.selection().len(), 4);
    (h, ids)
}

/// True when `actual` traces the same curve as `expected`, within tolerance.
fn assert_same(actual: &Entity, expected: &Entity) {
    let close = |a: Point2, b: Point2| a.distance_to(b) < 1e-9;
    let ok = match (actual, expected) {
        (Entity::Line(a), Entity::Line(b)) => close(a.start, b.start) && close(a.end, b.end),
        (Entity::Circle(a), Entity::Circle(b)) => {
            close(a.center, b.center) && (a.radius - b.radius).abs() < 1e-9
        }
        (Entity::Arc(a), Entity::Arc(b)) => {
            let (a, b) = (a.arc(), b.arc());
            close(a.center, b.center)
                && close(a.start_point(), b.start_point())
                && close(a.mid_point(), b.mid_point())
                && close(a.end_point(), b.end_point())
        }
        (Entity::Polyline(a), Entity::Polyline(b)) => {
            a.closed == b.closed
                && a.vertices.len() == b.vertices.len()
                && a.vertices
                    .iter()
                    .zip(&b.vertices)
                    .all(|(a, b)| close(*a, *b))
        }
        _ => false,
    };
    assert!(ok, "expected {expected:?}, got {actual:?}");
}

fn assert_all_same(h: &Harness, expected: &[Entity]) {
    let actual = h.entities();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_same(actual, expected);
    }
}

// ── ROTATE (#93) ──

/// Every M1 entity type, including arc angle updates.
#[test]
fn rotate_turns_every_kind_of_entity_about_the_base_point() {
    let (mut h, ids) = everything_selected();
    h.enter("RO");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify rotation angle:");
    h.enter("90");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.ids(), ids, "entities keep their IDs");
    assert_all_same(
        &h,
        &[
            LineEntity::new(p(0.0, 0.0), p(0.0, 100.0)).into(),
            CircleEntity::new(p(-50.0, 50.0), 20.0).into(),
            // The quarter arc now runs from (0, 140) to (-40, 100): 90° to 180°.
            ArcEntity::new(p(0.0, 100.0), 40.0, FRAC_PI_2, PI).into(),
            PolylineEntity::new(vec![p(-60.0, 0.0), p(-60.0, 40.0), p(-100.0, 40.0)], false).into(),
        ],
    );
    let Entity::Arc(arc) = &h.entities()[2] else {
        panic!("an arc stays an arc");
    };
    assert!((arc.start_angle - FRAC_PI_2).abs() < 1e-12 && (arc.end_angle - PI).abs() < 1e-12);
    assert!(h.session.selection().is_empty());
}

#[test]
fn rotating_a_quarter_turn_is_exact() {
    let mut h = Harness::with_entities([line()]);
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["ROTATE", "100,0", "90"]);
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(100.0, -100.0), p(100.0, 0.0))]
    );
}

/// Rotating by 360° reproduces the original geometry within tolerance.
#[test]
fn a_full_turn_reproduces_the_original_geometry() {
    let (mut h, _) = everything_selected();
    h.enter_all(&["RO", "37,-12", "360"]);
    assert_all_same(&h, &[line(), circle(), arc(), polyline()]);

    // And so do five turns of 72°.
    for _ in 0..5 {
        h.select_box(p(-250.0, -250.0), p(300.0, 280.0));
        h.enter_all(&["RO", "37,-12", "72"]);
    }
    assert_all_same(&h, &[line(), circle(), arc(), polyline()]);
}

#[test]
fn the_rotation_angle_can_be_given_with_the_pointer() {
    let mut h = Harness::with_entities([line()]);
    h.toggle(Toggle::Osnap);
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["RO", "0,0"]);
    // Straight up from the base point: 90°.
    h.click_world(p(0.0, 75.0));
    assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(0.0, 100.0))]);
}

#[test]
fn rotate_previews_without_touching_the_document_and_commits_once() {
    let mut h = Harness::with_entities([line()]);
    h.toggle(Toggle::Osnap);
    let id = h.ids()[0];
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["RO", "0,0"]);
    let revision = h.session.drawing().revision();

    for step in 1..50 {
        let dirty = h.move_to_world(p(100.0, f64::from(step)));
        assert!(!dirty.contains(Dirty::SCENE));
    }
    h.move_to_world(p(0.0, 60.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::PreviewTransformed {
                ids: vec![id],
                transform: Transform::rotation(p(0.0, 0.0), 90.0)
            })
    );
    assert_eq!(h.session.drawing().revision(), revision);

    h.enter("45");
    h.undo();
    assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(100.0, 0.0))]);
    assert!(!h.session.ui_state().can_undo, "one command, one undo step");
}

#[test]
fn rotate_asks_for_objects_when_nothing_is_selected() {
    let mut h = Harness::with_entities([line()]);
    h.enter("RO");
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "180"]);
    assert_same(
        &h.entities()[0],
        &LineEntity::new(p(0.0, 0.0), p(-100.0, 0.0)).into(),
    );

    let mut h = Harness::new();
    h.enter_all(&["RO", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.log().last().unwrap(), "No objects selected.");
}

/// DESIGN §18.2: what is being rotated is not a snap target at its old place.
#[test]
fn entities_being_rotated_or_scaled_are_not_snap_targets() {
    for command in ["RO", "SC"] {
        let mut h = Harness::with_entities([line(), circle()]);
        h.click_world(p(50.0, 0.0));
        h.enter_all(&[command, "0,-80"]);
        h.move_to_world(p(99.0, 1.0));
        assert_eq!(h.session.snap(), None, "{command}: its own endpoint");
        h.move_to_world(p(51.0, 49.0));
        assert!(h.session.snap().is_some(), "{command}: the circle's centre");
    }
}

// ── SCALE (#94) ──

#[test]
fn scale_scales_every_kind_of_entity_about_the_base_point() {
    let (mut h, ids) = everything_selected();
    h.enter("SC");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify scale factor:");
    h.enter("0.5");

    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.ids(), ids);
    assert_eq!(
        h.entities(),
        vec![
            LineEntity::new(p(0.0, 0.0), p(50.0, 0.0)).into(),
            CircleEntity::new(p(25.0, 25.0), 10.0).into(),
            ArcEntity::new(p(50.0, 0.0), 20.0, 0.0, FRAC_PI_2).into(),
            PolylineEntity::new(vec![p(0.0, 30.0), p(20.0, 30.0), p(20.0, 50.0)], false).into(),
        ]
    );
}

#[test]
fn scale_works_from_any_base_point() {
    let mut h = Harness::with_entities([circle()]);
    h.click_world(p(70.0, 50.0));
    h.enter_all(&["SCALE", "70,50", "3"]);
    // The point of the circle on the base point stays put.
    assert_eq!(
        h.entities(),
        vec![CircleEntity::new(p(10.0, 50.0), 60.0).into()]
    );
}

/// Zero and negative factors are rejected with a message.
#[test]
fn zero_and_negative_scale_factors_are_refused_and_asked_again() {
    let mut h = Harness::with_entities([line()]);
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["SC", "0,0"]);
    let revision = h.session.drawing().revision();

    for bad in ["0", "-2"] {
        h.enter(bad);
        assert_eq!(
            h.log().last().unwrap(),
            "The scale factor must be greater than zero."
        );
        assert_eq!(h.prompt(), "Specify scale factor:", "after {bad}");
    }
    assert_eq!(h.session.drawing().revision(), revision);

    h.enter("2");
    assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(200.0, 0.0))]);
}

#[test]
fn the_scale_factor_can_be_given_with_the_pointer_as_a_distance() {
    let mut h = Harness::with_entities([line()]);
    h.toggle(Toggle::Osnap);
    let id = h.ids()[0];
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["SC", "0,0"]);

    h.move_to_world(p(0.0, 1.5));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::PreviewTransformed {
                ids: vec![id],
                transform: Transform::scaling(p(0.0, 0.0), 1.5).unwrap()
            })
    );
    h.click_world(p(0.0, 1.5));
    assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(150.0, 0.0))]);

    h.undo();
    assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(100.0, 0.0))]);
}

// ── MIRROR (#95) ──

#[test]
fn mirror_keeps_the_originals_by_default_and_adds_reflections() {
    let (mut h, ids) = everything_selected();
    h.enter("MI");
    assert_eq!(h.prompt(), "Specify first point of mirror line:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify second point of mirror line:");
    h.enter("0,10");
    assert_eq!(h.prompt(), "Erase source objects? [Yes/No] <No>:");
    h.enter("");

    assert_eq!(h.prompt(), "Command:");
    let entities = h.entities();
    assert_eq!(entities.len(), 8);
    assert_eq!(&h.ids()[..4], &ids[..], "the originals are untouched");
    assert_eq!(&entities[..4], &[line(), circle(), arc(), polyline()]);
    // Reflected across the Y axis.
    let expected: [Entity; 4] = [
        LineEntity::new(p(0.0, 0.0), p(-100.0, 0.0)).into(),
        CircleEntity::new(p(-50.0, 50.0), 20.0).into(),
        // (140,0)→(100,40) becomes (-100,40)→(-140,0): 90° to 180° about (-100, 0).
        ArcEntity::new(p(-100.0, 0.0), 40.0, FRAC_PI_2, PI).into(),
        PolylineEntity::new(vec![p(0.0, 60.0), p(-40.0, 60.0), p(-40.0, 100.0)], false).into(),
    ];
    for (actual, expected) in entities[4..].iter().zip(&expected) {
        assert_same(actual, expected);
    }

    h.undo();
    assert_eq!(h.entities().len(), 4, "one undo removes every reflection");
}

#[test]
fn mirror_can_erase_the_originals() {
    let (mut h, ids) = everything_selected();
    h.enter_all(&["MIRROR", "0,0", "0,10", "Y"]);
    assert_eq!(h.ids(), ids, "the same entities, reflected in place");
    assert_same(
        &h.entities()[0],
        &LineEntity::new(p(0.0, 0.0), p(-100.0, 0.0)).into(),
    );
    assert_same(
        &h.entities()[1],
        &CircleEntity::new(p(-50.0, 50.0), 20.0).into(),
    );

    h.undo();
    assert_eq!(h.entities(), vec![line(), circle(), arc(), polyline()]);
}

/// Arcs keep a valid counter-clockwise representation.
#[test]
fn a_mirrored_arc_is_the_reflection_not_its_complement() {
    let mut h = Harness::with_entities([arc()]);
    h.click_world(p(140.0, 0.0));
    h.enter_all(&["MI", "0,0", "10,0", "Y"]); // across the X axis
    let Entity::Arc(arc) = &h.entities()[0] else {
        panic!("an arc stays an arc");
    };
    // (140,0)→(100,40) becomes (100,-40)→(140,0), counter-clockwise: 270° to 360°.
    let image = arc.arc();
    assert!(
        (image.sweep - FRAC_PI_2).abs() < 1e-12,
        "sweep {}",
        image.sweep
    );
    assert!(image.start_point().distance_to(p(100.0, -40.0)) < 1e-9);
    assert!(image.end_point().distance_to(p(140.0, 0.0)) < 1e-9);
}

/// Mirroring twice across the same line reproduces the original geometry.
#[test]
fn mirroring_twice_across_the_same_line_reproduces_the_original() {
    let (mut h, _) = everything_selected();
    h.enter_all(&["MI", "-20,-30", "70,110", "Y"]);
    h.select_box(p(-390.0, -290.0), p(390.0, 290.0));
    assert_eq!(h.session.selection().len(), 4);
    h.enter_all(&["MI", "-20,-30", "70,110", "Y"]);
    assert_all_same(&h, &[line(), circle(), arc(), polyline()]);
}

#[test]
fn a_mirror_line_needs_two_different_points() {
    let mut h = Harness::with_entities([line()]);
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["MI", "5,5", "5,5"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The mirror line needs two different points."
    );
    assert_eq!(h.prompt(), "Specify second point of mirror line:");
    h.enter_all(&["5,6", "N"]);
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn mirror_previews_the_reflection_while_the_line_is_being_placed() {
    let mut h = Harness::with_entities([line()]);
    h.toggle(Toggle::Osnap);
    let id = h.ids()[0];
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["MI", "0,-50"]);
    let revision = h.session.drawing().revision();

    h.move_to_world(p(0.0, 50.0));
    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::PreviewSegment {
        start: p(0.0, -50.0),
        end: p(0.0, 50.0)
    }));
    assert!(overlay.contains(&OverlayItem::PreviewTransformed {
        ids: vec![id],
        transform: Transform::mirror(p(0.0, -50.0), p(0.0, 50.0)).unwrap()
    }));
    assert_eq!(h.session.drawing().revision(), revision);

    // Still previewed while the question is open; Escape leaves no trace.
    h.click_world(p(0.0, 50.0));
    assert!(
        h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::PreviewTransformed { .. }))
    );
    h.escape();
    assert_eq!(h.entities(), vec![line()]);
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn ortho_makes_the_mirror_line_axis_parallel() {
    let mut h = Harness::with_entities([line()]);
    h.toggle(Toggle::Osnap);
    h.toggle(Toggle::Ortho);
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["MI", "0,0"]);
    h.click_world(p(7.0, 90.0)); // constrained to straight up
    h.enter("Y");
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(0.0, 0.0), p(-100.0, 0.0))]
    );
}
