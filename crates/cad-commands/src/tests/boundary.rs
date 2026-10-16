//! BOUNDARY through the session (#229): HATCH's loop drawn as a closed
//! polyline — of lines and an arc, a circle, with islands or without —
//! the area HATCH fills, a point outside every loop refused, one undo.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, HatchEntity, LineEntity, PolylineEntity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn polylines(h: &Harness) -> Vec<PolylineEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Polyline(polyline) => Some(polyline),
            _ => None,
        })
        .collect()
}

/// A slot: two lines and the ends joined by a line and a half circle.
fn slot() -> Vec<Entity> {
    vec![
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        ArcEntity::new(p(100.0, 30.0), 30.0, -FRAC_PI_2, FRAC_PI_2).into(),
        LineEntity::new(p(100.0, 60.0), p(0.0, 60.0)).into(),
        LineEntity::new(p(0.0, 60.0), p(0.0, 0.0)).into(),
        CircleEntity::new(p(40.0, 30.0), 10.0).into(),
    ]
}

/// The hatch HATCH makes at `at` in the same drawing.
fn hatched(entities: Vec<Entity>, at: &str) -> HatchEntity {
    let mut h = Harness::with_entities(entities);
    h.enter_all(&["HATCH", at]);
    let hatch: Vec<HatchEntity> = h
        .entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .collect();
    hatch[0].clone()
}

#[test]
fn a_loop_of_lines_and_an_arc_with_its_island_is_hatch_s_region() {
    let mut h = Harness::with_entities(slot());
    h.enter("BO");
    assert_eq!(h.prompt(), "Pick internal point or [Islands]:");
    h.enter("10,30");
    assert!(said(&h, "BOUNDARY created 2 polylines."));
    let made = polylines(&h);
    assert_eq!(made.len(), 2);
    let outer = &made[0];
    assert!(outer.closed && outer.has_arcs(), "its arc a bulge");
    let island = &made[1];
    // They are the loops HATCH fills between there: the same region,
    // whose area is theirs exactly — the fill drawn follows the arcs by
    // chords, so its own is a hair less.
    let hatch = hatched(slot(), "10,30");
    assert_eq!(hatch.loops, vec![outer.path(), island.path()]);
    let between = outer.path().area().abs() - island.path().area().abs();
    assert!((between - (6000.0 + PI * 450.0 - PI * 100.0)).abs() < 1e-6);
    assert!(
        (between - hatch.area()).abs() < between * 1e-3,
        "{between} vs {}",
        hatch.area()
    );
    h.enter("");
    h.undo();
    assert!(polylines(&h).is_empty(), "one undo");
}

#[test]
fn islands_off_draws_the_outer_loop_and_a_circle_is_a_loop_of_its_own() {
    let mut h = Harness::with_entities(slot());
    h.enter_all(&["BOUNDARY", "I"]);
    assert!(said(&h, "Islands are not detected."));
    h.enter("10,30");
    assert_eq!(polylines(&h).len(), 1);
    // Inside the circle: the circle, as a closed polyline of two arcs.
    h.enter("40,30");
    let made = polylines(&h);
    assert_eq!(made.len(), 2);
    let circle = &made[1];
    assert_eq!(circle.vertices.len(), 2);
    assert!((circle.path().area().abs() - PI * 100.0).abs() < 1e-9);
    assert_eq!(
        hatched(slot(), "40,30").loops,
        vec![circle.path()],
        "what HATCH fills"
    );
    h.enter("");
    h.undo();
    assert!(polylines(&h).is_empty(), "both picks, one undo");
}

#[test]
fn a_point_outside_every_loop_is_refused() {
    let mut h = Harness::with_entities(slot());
    h.enter_all(&["BO", "500,500"]);
    assert!(said(&h, "No closed boundary surrounds that point."));
    assert_eq!(h.prompt(), "Pick internal point or [Islands]:");
    assert!(polylines(&h).is_empty());
}
