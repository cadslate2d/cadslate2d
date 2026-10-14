//! POLYGON (#201): a regular polygon by centre and radius, inscribed or
//! circumscribed, or by an edge; a closed polyline on the current layer.

use cad_core::Point2;
use cad_document::Entity;
use proptest::prelude::*;

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn polygon(h: &Harness) -> Vec<Point2> {
    match &h.entities()[0] {
        Entity::Polyline(polyline) => {
            assert!(polyline.closed);
            polyline.vertices.clone()
        }
        other => panic!("not a polyline: {other:?}"),
    }
}

#[test]
fn polygon_by_centre_and_radius_inscribed_circumscribed_or_by_an_edge() {
    let mut h = Harness::new();
    h.enter_all(&["LAYER", "N", "WALLS", "S", "WALLS", ""]);
    h.enter("POLYGON");
    assert_eq!(h.prompt(), "Enter number of sides <4>:");
    h.enter("2");
    assert_eq!(last(&h), "Enter a whole number of sides from 3 to 1024.");
    h.enter("6");
    assert_eq!(h.prompt(), "Specify center of polygon or [Edge]:");
    h.enter("0,0");
    assert_eq!(
        h.prompt(),
        "Enter an option or [Inscribed in circle/Circumscribed about circle] <Inscribed in circle>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Specify radius of circle:");
    h.enter("10");
    assert_eq!(h.prompt(), "Command:");
    let corners = polygon(&h);
    assert_eq!(corners.len(), 6);
    assert!(corners[0].distance_to(p(10.0, 0.0)) < 1e-9);
    for corner in &corners {
        assert!((corner.distance_to(Point2::ORIGIN) - 10.0).abs() < 1e-9);
    }
    let walls = h.session.drawing().layers().find("WALLS").unwrap();
    assert_eq!(h.properties()[0].layer, walls);
    h.undo();

    // Circumscribed, the radius point is the middle of a side.
    h.enter_all(&["POL", "6", "0,0", "C", "10,0"]);
    let corners = polygon(&h);
    assert_eq!(corners.len(), 6);
    let middle = corners[0] + (corners[1] - corners[0]) * 0.5;
    assert!(middle.distance_to(p(10.0, 0.0)) < 1e-9, "{corners:?}");
    h.undo();

    // By an edge, turning counter-clockwise.
    h.enter_all(&["POL", "4", "E", "0,0", "10,0"]);
    let corners = polygon(&h);
    assert!(corners[2].distance_to(p(10.0, 10.0)) < 1e-9);
    assert!(corners[3].distance_to(p(0.0, 10.0)) < 1e-9);
    h.undo();
    h.enter_all(&["POL", "4", "E", "0,0", "0,0"]);
    assert_eq!(last(&h), "The edge needs two different points.");
    h.escape();
}

#[test]
fn polygon_is_previewed_as_the_pointer_would_make_it() {
    let mut h = Harness::new();
    h.enter_all(&["POLYGON", "3", "0,0", ""]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(10.0, 0.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Polyline(polyline)) if polyline.vertices.len() == 3
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.click_world(p(10.0, 0.0));
    assert_eq!(polygon(&h).len(), 3);
}

proptest! {
    /// #201: every corner is on the circle — at the radius inscribed, at
    /// the sides' reach circumscribed, where each side's middle is on the
    /// circle — and the sides are equal.
    #[test]
    fn every_corner_is_on_the_circle(
        sides in 3..13usize, cx in -100.0..100.0_f64, cy in -100.0..100.0_f64,
        radius in 1.0..100.0_f64, inscribed in any::<bool>(),
    ) {
        let mut h = Harness::new();
        h.enter_all(&[
            "POLYGON",
            &sides.to_string(),
            &format!("{cx},{cy}"),
            if inscribed { "I" } else { "C" },
            &format!("{radius}"),
        ]);
        let corners = polygon(&h);
        prop_assert_eq!(corners.len(), sides);
        let center = p(cx, cy);
        let reach = if inscribed { radius } else { radius / (std::f64::consts::PI / sides as f64).cos() };
        for corner in &corners {
            prop_assert!((corner.distance_to(center) - reach).abs() < 1e-6);
        }
        let side = corners[0].distance_to(corners[1]);
        for i in 0..sides {
            let next = corners[(i + 1) % sides];
            prop_assert!((corners[i].distance_to(next) - side).abs() < 1e-6);
            if !inscribed {
                let middle = corners[i] + (next - corners[i]) * 0.5;
                prop_assert!((middle.distance_to(center) - radius).abs() < 1e-6);
            }
        }
    }
}
