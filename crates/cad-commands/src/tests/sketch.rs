//! SKETCH (#326) through the session: the pen down and up by clicks, a
//! vertex each record increment of the pointer's path, strokes kept as
//! polylines on Enter in one undo, nothing on Escape, ORTHO shaping it.

use cad_core::Point2;
use cad_document::Entity;

use crate::testing::Harness;
use crate::{KeyAction, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn polylines(h: &Harness) -> Vec<Vec<Point2>> {
    h.entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Polyline(polyline) if !polyline.closed => Some(polyline.vertices),
            _ => None,
        })
        .collect()
}

fn close(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-6
}

#[test]
fn strokes_are_recorded_every_increment_and_kept_as_polylines_in_one_undo() {
    let mut h = Harness::new();
    h.enter_all(&["SKETCH", "I"]);
    assert!(h.prompt().starts_with("Specify record increment <"));
    h.enter("1");
    assert_eq!(
        h.prompt(),
        "Click to put the pen down, or press Enter to keep the sketch or [Increment]:"
    );
    // Moves before the pen is down record nothing.
    h.move_to_world(p(-5.0, 0.0));
    h.click_world(p(0.0, 0.0));
    assert!(h.prompt().starts_with("Sketching: click to lift the pen"));
    for x in [0.4, 1.0, 1.5, 2.2, 3.0] {
        h.move_to_world(p(x, 0.0));
    }
    h.click_world(p(3.5, 0.0));
    // A second stroke.
    h.click_world(p(10.0, 10.0));
    h.move_to_world(p(10.0, 12.0));
    h.click_world(p(10.0, 13.0));
    let revision = h.session.drawing().revision();
    assert!(polylines(&h).is_empty(), "nothing until Enter");
    h.enter("");
    assert_eq!(h.log().last().unwrap(), "2 sketched polylines recorded.");
    assert_ne!(h.session.drawing().revision(), revision);
    let strokes = polylines(&h);
    assert_eq!(strokes.len(), 2);
    let first = strokes.iter().find(|s| close(s[0], p(0.0, 0.0))).unwrap();
    // 0.4 is under the increment; 1.0, 2.2 (1.2 on), and 3.5 (where the
    // pen lifted) are kept; 1.5 and 3.0 are within one of the last.
    let xs: Vec<f64> = first.iter().map(|v| v.x).collect();
    assert_eq!(xs.len(), 4, "{xs:?}");
    for (x, want) in xs.iter().zip([0.0, 1.0, 2.2, 3.5]) {
        assert!((x - want).abs() < 1e-6, "{xs:?}");
    }
    h.undo();
    assert!(polylines(&h).is_empty(), "one undo");
}

#[test]
fn escape_keeps_nothing_and_a_stroke_of_one_point_is_not_kept() {
    let mut h = Harness::new();
    h.enter("SKETCH");
    h.click_world(p(0.0, 0.0));
    h.move_to_world(p(50.0, 0.0));
    h.session.key(KeyAction::Escape);
    assert_eq!(h.prompt(), "Command:");
    assert!(polylines(&h).is_empty());
    // Down and up at one place: nothing to keep.
    h.enter("SKETCH");
    h.click_world(p(5.0, 5.0));
    h.click_world(p(5.0, 5.0));
    h.enter("");
    assert_eq!(h.log().last().unwrap(), "Nothing sketched.");
    assert!(polylines(&h).is_empty());
}

#[test]
fn ortho_shapes_the_path_from_the_last_vertex() {
    let mut h = Harness::new();
    h.session.key(KeyAction::Toggle(Toggle::Ortho));
    h.enter_all(&["SKETCH", "I", "1"]);
    h.click_world(p(0.0, 0.0));
    // Mostly along x: kept on the x axis.
    h.move_to_world(p(5.0, 0.4));
    h.move_to_world(p(5.3, 6.0));
    h.click_world(p(5.3, 6.0));
    h.enter("");
    let stroke = &polylines(&h)[0];
    assert!(close(stroke[1], p(5.0, 0.0)), "{stroke:?}");
    // Then mostly along y from there.
    assert!((stroke[2].x - 5.0).abs() < 1e-6, "{stroke:?}");
}
