//! REVCLOUD through the session (#215): each mode, the arc lengths kept
//! for the session, the objects it refuses, one undo, and the preview the
//! point commits.

use cad_core::Point2;
use cad_document::{CircleEntity, Color, Entity, LineEntity, PolylineEntity, Properties};
use cad_geometry::CLOUD_BULGE;

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn clouds(h: &Harness) -> Vec<PolylineEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Polyline(polyline) => Some(polyline),
            _ => None,
        })
        .collect()
}

#[test]
fn a_rectangular_cloud_is_one_closed_polyline_of_outward_arcs_and_one_undo() {
    let mut h = Harness::new();
    h.enter("REVCLOUD");
    assert!(said(&h, "Arc length: shortest 7.5, longest 15."));
    assert_eq!(
        h.prompt(),
        "Specify first corner point or [Arc length/Object/Rectangular/Polygonal/REVerse] <Object>:"
    );
    h.enter_all(&["0,0", "60,30"]);
    assert!(said(&h, "Revision cloud made: 12 arcs."));
    assert_eq!(h.prompt(), "Command:");
    let [cloud] = clouds(&h).try_into().unwrap();
    assert!(cloud.closed);
    assert_eq!(cloud.vertices.len(), 12);
    assert_eq!(cloud.bulges().collect::<Vec<_>>(), vec![CLOUD_BULGE; 12]);
    h.undo();
    assert!(clouds(&h).is_empty());
}

#[test]
fn arc_length_and_reverse_shape_the_arcs_and_the_lengths_are_kept() {
    let mut h = Harness::new();
    h.enter_all(&["REVCLOUD", "A"]);
    assert_eq!(h.prompt(), "Specify shortest arc <7.5>:");
    h.enter("5");
    assert_eq!(h.prompt(), "Specify longest arc <15>:");
    h.enter("4");
    assert!(said(
        &h,
        "The longest arc cannot be shorter than the shortest."
    ));
    h.enter("10");
    assert!(said(&h, "Arc length: shortest 5, longest 10."));
    h.enter("REV");
    assert!(said(&h, "The arcs bow inwards."));
    h.enter_all(&["0,0", "60,30"]);
    assert!(said(&h, "Revision cloud made: 18 arcs."));
    assert_eq!(
        clouds(&h)[0].bulges().collect::<Vec<_>>(),
        vec![-CLOUD_BULGE; 18]
    );
    // The next REVCLOUD starts from the lengths set.
    h.enter("REVCLOUD");
    assert!(said(&h, "Arc length: shortest 5, longest 10."));
    h.escape();
}

#[test]
fn a_polygonal_cloud_closes_on_enter_after_three_points() {
    let mut h = Harness::new();
    h.enter_all(&["REVCLOUD", "P"]);
    assert_eq!(
        h.prompt(),
        "Specify start point or [Arc length/Object/Rectangular/Polygonal/REVerse] <Object>:"
    );
    h.enter_all(&["0,0", "60,0", ""]);
    assert!(said(&h, "A cloud needs three points at least."));
    h.enter_all(&["90,40", "U", "30,45"]);
    assert_eq!(h.prompt(), "Specify next point or [Undo] <close>:");
    h.enter("");
    // 60 and two sides of 54.08: four arcs each.
    assert!(said(&h, "Revision cloud made: 12 arcs."));
    let cloud = &clouds(&h)[0];
    assert!(cloud.vertices.contains(&p(30.0, 45.0)));
    assert!(!cloud.vertices.contains(&p(90.0, 40.0)), "undone");
}

#[test]
fn object_turns_a_closed_outline_into_a_cloud_in_its_place() {
    let red = Properties {
        color: Color::Index(1),
        ..Properties::default()
    };
    let mut drawing = cad_document::Drawing::new();
    let mut tx = drawing.transaction();
    let circle = tx
        .insert_with(CircleEntity::new(p(0.0, 0.0), 20.0), red)
        .unwrap();
    let wide = PolylineEntity::new(
        vec![p(100.0, 0.0), p(160.0, 0.0), p(160.0, 30.0), p(100.0, 30.0)],
        true,
    )
    .with_width(2.0);
    let polyline = tx.insert(wide).unwrap();
    tx.insert(PolylineEntity::new(
        vec![p(0.0, 100.0), p(50.0, 100.0)],
        false,
    ))
    .unwrap();
    tx.insert(LineEntity::new(p(0.0, 150.0), p(50.0, 150.0)))
        .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);

    h.enter_all(&["REVCLOUD", ""]);
    assert_eq!(h.prompt(), "Select a closed object to turn into a cloud:");
    h.click_world(p(25.0, 100.0));
    assert!(said(
        &h,
        "The polyline is open: a cloud needs a closed outline."
    ));
    h.click_world(p(25.0, 150.0));
    assert!(said(
        &h,
        "A line is not a closed outline: pick a closed polyline, a circle, an ellipse, or a closed spline."
    ));
    h.click_world(p(20.0, 0.0));
    assert_eq!(h.prompt(), "Command:");
    let record = h.session.drawing().record(circle).unwrap();
    let Entity::Polyline(cloud) = &*record.entity else {
        panic!("the circle is a cloud now");
    };
    assert_eq!(record.properties, red, "its properties kept");
    // A circle of radius 20, as two half turns: five arcs each, the
    // fewest whose chords are at most 15.
    assert_eq!(cloud.vertices.len(), 10);
    assert!(
        cloud
            .vertices
            .iter()
            .all(|vertex| (vertex.distance_to(Point2::ORIGIN) - 20.0).abs() < 1e-9)
    );

    // A polyline keeps its width.
    h.enter_all(&["REVCLOUD", "O"]);
    h.click_world(p(130.0, 0.0));
    let Entity::Polyline(cloud) = &*h.session.drawing().record(polyline).unwrap().entity else {
        panic!("a polyline");
    };
    assert_eq!(cloud.width(), 2.0);
    assert_eq!(cloud.vertices.len(), 12);
    h.undo();
    assert_eq!(
        h.session.drawing().entity(polyline),
        Some(
            &PolylineEntity::new(
                vec![p(100.0, 0.0), p(160.0, 0.0), p(160.0, 30.0), p(100.0, 30.0)],
                true
            )
            .with_width(2.0)
            .into()
        )
    );
}

#[test]
fn the_preview_is_the_cloud_the_corner_commits() {
    let mut h = Harness::new();
    h.enter_all(&["REVCLOUD", "0,0"]);
    h.move_to_world(p(45.0, 35.0));
    let shown = h
        .session
        .overlay()
        .into_iter()
        .find_map(|item| match item {
            OverlayItem::PreviewEntity(Entity::Polyline(cloud)) => Some(cloud),
            _ => None,
        })
        .expect("a cloud previewed");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(46.0, 35.0));
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter("45,35");
    assert_eq!(clouds(&h)[0], shown);
}
