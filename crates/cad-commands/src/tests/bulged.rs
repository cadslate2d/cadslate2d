//! Polylines with arc segments (#114), through the session: everything that
//! reads geometry sees the arcs, with no polyline-specific code in commands.

use cad_core::Point2;
use cad_document::{Entity, LineEntity, PolylineEntity};
use cad_geometry::Primitive;

use crate::testing::Harness;
use crate::{OverlayItem, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A slot 200 long and 80 high with semicircular ends of radius 40: the
/// right end bulges out to x = 240, the left to x = -40.
fn slot() -> PolylineEntity {
    PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(200.0, 0.0), p(200.0, 80.0), p(0.0, 80.0)],
        vec![0.0, 1.0, 0.0, 1.0],
        true,
    )
}

fn polyline_of(h: &Harness, index: usize) -> PolylineEntity {
    match &h.entities()[index] {
        Entity::Polyline(polyline) => polyline.clone(),
        other => panic!("expected a polyline, got {other:?}"),
    }
}

#[test]
fn a_click_on_an_arc_segment_selects_the_polyline_and_one_on_its_chord_does_not() {
    let mut h = Harness::with_entities([Entity::from(slot())]);
    h.click_world(p(200.0, 40.0)); // the chord of the right end: empty space
    assert!(h.session.selection().is_empty());
    h.escape();

    h.click_world(p(240.0, 40.0)); // the arc itself
    assert_eq!(h.session.selection().len(), 1);
    assert_eq!(h.session.grips().len(), 4, "one grip per vertex");
}

#[test]
fn a_crossing_box_over_the_bulge_selects_the_polyline() {
    let mut h = Harness::with_entities([Entity::from(slot())]);
    h.select_box(p(250.0, 50.0), p(230.0, 30.0)); // right to left: crossing
    assert_eq!(h.session.selection().len(), 1);
}

#[test]
fn rotate_and_mirror_keep_the_slot_a_slot() {
    let mut h = Harness::with_entities([Entity::from(slot())]);
    h.click_world(p(100.0, 0.0));
    h.enter_all(&["RO", "0,0", "90"]);
    let turned = polyline_of(&h, 0);
    assert_eq!(
        turned.vertices,
        vec![p(0.0, 0.0), p(0.0, 200.0), p(-80.0, 200.0), p(-80.0, 0.0)]
    );
    assert_eq!(
        turned.bulges().collect::<Vec<_>>(),
        vec![0.0, 1.0, 0.0, 1.0]
    );
    // The far end still bulges outwards: its middle is 40 beyond the vertices.
    assert!(Entity::from(turned).distance_to(p(-40.0, 240.0), cad_document::empty_blocks()) < 1e-9);

    h.click_world(p(0.0, 100.0));
    h.enter_all(&["MI", "0,0", "0,10", "Y"]);
    let mirrored = polyline_of(&h, 0);
    assert_eq!(
        mirrored.bulges().collect::<Vec<_>>(),
        vec![0.0, -1.0, 0.0, -1.0]
    );
    assert!(
        Entity::from(mirrored).distance_to(p(40.0, 240.0), cad_document::empty_blocks()) < 1e-9,
        "still bulging outwards"
    );
}

#[test]
fn a_vertex_grip_reshapes_the_arcs_that_meet_there() {
    let mut h = Harness::with_entities([Entity::from(slot())]);
    h.toggle(Toggle::Osnap);
    h.click_world(p(100.0, 0.0));
    h.click_world(p(200.0, 80.0)); // the grip of vertex 2

    // The preview is the two segments at that vertex, arc included.
    h.move_to_world(p(200.0, 120.0));
    assert!(
        h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::PreviewGrip { .. }))
    );
    h.enter("200,120");

    let edited = polyline_of(&h, 0);
    assert_eq!(edited.vertices[2], p(200.0, 120.0));
    assert_eq!(
        edited.bulges().collect::<Vec<_>>(),
        vec![0.0, 1.0, 0.0, 1.0]
    );
    let Some(Primitive::Arc(end)) = edited.primitives().nth(1) else {
        panic!("the right end is still an arc");
    };
    assert!(
        (end.radius - 60.0).abs() < 1e-9,
        "a semicircle over the new chord of 120"
    );
    h.undo();
    assert_eq!(polyline_of(&h, 0), slot());
}

#[test]
fn an_arc_segment_is_a_cutting_edge_and_a_boundary() {
    // A horizontal line through the slot, sticking out of both ends.
    let through: Entity = LineEntity::new(p(-100.0, 40.0), p(300.0, 40.0)).into();
    let mut h = Harness::with_entities([Entity::from(slot()), through]);
    h.enter("TR");
    h.click_world(p(100.0, 0.0)); // the slot is the cutting edge
    h.enter("");
    h.click_world(p(280.0, 40.0)); // the part beyond the right end
    h.enter("");
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(-100.0, 40.0), p(240.0, 40.0))]
    );

    // And a line stopping short is extended to the arc of the left end.
    let short: Entity = LineEntity::new(p(100.0, 60.0), p(20.0, 60.0)).into();
    let mut h = Harness::with_entities([Entity::from(slot()), short]);
    h.enter("EX");
    h.click_world(p(100.0, 0.0));
    h.enter("");
    h.click_world(p(25.0, 60.0));
    h.enter("");
    // The left end is the circle about (0, 40) of radius 40: at y = 60, x = -√1200.
    let end = h.lines()[0].end;
    assert!(
        end.distance_to(p(-(1200f64.sqrt()), 60.0)) < 1e-9,
        "{end:?}"
    );
}

#[test]
fn snapping_finds_the_centre_of_an_arc_segment() {
    let mut h = Harness::with_entities([Entity::from(slot())]);
    h.enter("C");
    h.click_world(p(202.0, 41.0));
    h.enter("10");
    let Entity::Circle(circle) = &h.entities()[1] else {
        panic!("expected a circle");
    };
    assert_eq!(circle.center, p(200.0, 40.0));
}
