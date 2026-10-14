//! Points (#198): POINT, DIVIDE, MEASURE, PTYPE; the NODe snap; a point
//! picked, selected by a window, and moved by its grip.

use std::f64::consts::PI;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, Path, PointStyle, PolylineEntity};
use proptest::prelude::*;

use crate::testing::Harness;
use crate::{Dirty, SnapMode};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// The points of the drawing, in ID order.
fn points(h: &Harness) -> Vec<Point2> {
    let mut ids: Vec<_> = h
        .session
        .drawing()
        .shown()
        .filter_map(|(id, entity)| match entity {
            Entity::Point(point) => Some((id, point.position)),
            _ => None,
        })
        .collect();
    ids.sort_by_key(|(id, _)| *id);
    ids.into_iter().map(|(_, at)| at).collect()
}

#[test]
fn point_places_a_point_at_each_place_until_enter_and_the_command_is_one_undo() {
    let mut h = Harness::new();
    h.enter("POINT");
    assert_eq!(h.prompt(), "Specify a point:");
    let dirty = h.enter("10,10");
    assert!(dirty.contains(Dirty::SCENE));
    h.enter_all(&["20,5", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(points(&h), vec![p(10.0, 10.0), p(20.0, 5.0)]);
    h.undo();
    assert!(points(&h).is_empty(), "the command's points go as one");
    h.redo();
    h.enter_all(&["PO", "@5,5", ""]);
    assert_eq!(points(&h), vec![p(10.0, 10.0), p(20.0, 5.0), p(25.0, 10.0)]);
    h.enter_all(&["LIST", "ALL", ""]);
    assert!(
        h.log()
            .iter()
            .any(|line| line.starts_with("POINT  Layer: \"0\""))
    );
    assert_eq!(last(&h), "  Position: 25.0000, 10.0000");
}

#[test]
fn ptype_sets_the_style_by_keyword_or_number_as_one_undoable_change_that_redraws() {
    let mut h = Harness::new();
    h.enter_all(&["PO", "0,0", ""]);
    let shown = h.session.drawing().shown_revision();
    h.enter("PTYPE");
    assert_eq!(
        h.prompt(),
        "Enter point mode, as a number or [Dot/Cross/X/Tick/Blank/CIrcle/SQuare/Plain] <0>:"
    );
    h.enter("X");
    assert_eq!(last(&h), "Point mode: X (3).");
    h.enter("CI");
    assert_eq!(last(&h), "Point mode: X in a circle (35).");
    h.enter("");
    assert_eq!(h.prompt(), "Enter point size <1>:");
    let dirty = h.enter("2.5");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(last(&h), "Point style: X in a circle (35), size 2.5.");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.session.drawing().settings().point_style,
        PointStyle::new(35, 2.5)
    );
    assert_ne!(
        h.session.drawing().shown_revision(),
        shown,
        "every point is redrawn"
    );
    // The scene draws the marker: an X of two strokes and a circle.
    assert!(h.session.drawing().entity(h.ids()[0]).is_some());

    // One undo takes the style back.
    h.undo();
    assert_eq!(
        h.session.drawing().settings().point_style,
        PointStyle::default()
    );

    // By number, with the refusals.
    h.enter_all(&["DDPTYPE", "5"]);
    assert!(last(&h).starts_with("A point mode is 0 (dot)"));
    h.enter("66");
    assert_eq!(h.prompt(), "Enter point size <1>:");
    h.enter("0");
    assert_eq!(last(&h), "the point size must be a positive number.");
    h.enter("4");
    assert_eq!(last(&h), "Point style: cross in a square (66), size 4.");
    // Enter keeps what there is.
    h.enter_all(&["PTYPE", "", ""]);
    assert_eq!(
        h.session.drawing().settings().point_style,
        PointStyle::new(66, 4.0)
    );
    assert_eq!(last(&h), "Point style: cross in a square (66), size 4.");
}

#[test]
fn divide_counts_along_a_curve_and_refuses_what_is_not_one_or_not_a_count() {
    let mut h = Harness::with_entities([
        cad_document::LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        cad_document::CircleEntity::new(p(200.0, 0.0), 10.0).into(),
        cad_document::TextEntity::new(p(0.0, 50.0), 5.0, 0.0, "no").into(),
    ]);
    h.enter_all(&["ZOOM", "E", "DIVIDE"]);
    assert_eq!(h.prompt(), "Select object to divide:");
    h.click_world(p(0.0, 50.0));
    assert!(last(&h).starts_with("That object cannot be divided or measured"));
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.prompt(), "Enter the number of segments:");
    h.enter("1");
    assert_eq!(last(&h), "Enter a whole number from 2 to 32767.");
    h.enter("2.5");
    assert_eq!(last(&h), "Enter a whole number from 2 to 32767.");
    h.enter("4");
    assert_eq!(last(&h), "Divided into 4 segments: 3 points placed.");
    assert_eq!(points(&h), vec![p(25.0, 0.0), p(50.0, 0.0), p(75.0, 0.0)]);
    // One undo takes them all; the line stays.
    h.undo();
    assert!(points(&h).is_empty());
    assert_eq!(h.session.drawing().shown().count(), 3);
    // A circle divided in four: its quadrants, from the rightmost.
    h.enter("DIV");
    h.click_world(p(210.0, 0.0));
    h.enter("4");
    assert_eq!(last(&h), "Divided into 4 segments: 4 points placed.");
    let tolerance = LinearTolerance::DEFAULT.around(&[p(200.0, 0.0)]);
    let placed = points(&h);
    for (got, want) in placed.iter().zip([
        p(210.0, 0.0),
        p(200.0, 10.0),
        p(190.0, 0.0),
        p(200.0, -10.0),
    ]) {
        assert!(tolerance.coincident(*got, want), "{got:?} is not {want:?}");
    }
}

#[test]
fn measure_steps_from_the_end_nearest_the_pick_and_takes_the_curves_layer() {
    let mut h = Harness::new();
    h.enter_all(&[
        "LAYER", "N", "NODES", "S", "NODES", "", "L", "0,0", "100,0", "",
    ]);
    h.enter_all(&["ZOOM", "E", "MEASURE"]);
    assert_eq!(h.prompt(), "Select object to measure:");
    h.click_world(p(95.0, 0.0));
    assert_eq!(h.prompt(), "Specify length of segment:");
    h.enter("-1");
    assert_eq!(last(&h), "The segment length must be a positive number.");
    h.enter("30");
    assert_eq!(last(&h), "Measured every 30: 3 points placed.");
    assert_eq!(points(&h), vec![p(70.0, 0.0), p(40.0, 0.0), p(10.0, 0.0)]);
    let layer = h
        .session
        .drawing()
        .layers()
        .get(h.properties()[1].layer)
        .unwrap();
    assert_eq!(layer.name, "NODES");
    h.undo();
    h.enter("ME");
    h.click_world(p(5.0, 0.0));
    h.enter("30");
    assert_eq!(points(&h), vec![p(30.0, 0.0), p(60.0, 0.0), p(90.0, 0.0)]);
    h.enter("ME");
    h.click_world(p(5.0, 0.0));
    h.enter("150");
    assert_eq!(last(&h), "The segment is longer than the object.");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn a_point_is_picked_at_its_place_taken_by_a_window_snapped_to_as_a_node_and_moved_by_its_grip() {
    let mut h = Harness::new();
    h.enter_all(&["PO", "50,50", ""]);
    // Picked at its place.
    h.click_world(p(50.0, 50.0));
    assert_eq!(h.session.selection_summary().headline(), "1 point");
    h.escape();
    assert!(h.session.selection().is_empty());
    // Taken by a window round it, not by one beside it.
    h.select_box(p(40.0, 40.0), p(60.0, 60.0));
    assert_eq!(h.session.selection().len(), 1);
    h.escape();
    h.select_box(p(60.0, 60.0), p(80.0, 80.0));
    assert!(h.session.selection().is_empty());
    // Snapped to as a node, on by default, with its own marker.
    assert!(h.session.snap_modes().is_on(SnapMode::Node));
    h.enter("L");
    h.move_to_world(p(51.0, 49.0));
    let state = h.session.ui_state();
    assert_eq!(state.snap, Some("Node"));
    assert_eq!(state.cursor, Some((50.0, 50.0)));
    h.click_world(p(51.0, 49.0));
    h.enter_all(&["100,100", ""]);
    assert_eq!(h.lines()[0].start, p(50.0, 50.0));
    // Its one grip moves it.
    h.click_world(p(50.0, 50.0));
    let grips = h.session.grips();
    assert_eq!(grips.len(), 1);
    assert_eq!(grips[0].position, p(50.0, 50.0));
    h.click_world(p(50.0, 50.0));
    h.enter("70,20");
    assert_eq!(points(&h), vec![p(70.0, 20.0)]);
}

proptest! {
    /// #198: DIVIDE and MEASURE land every point on the curve, at equal
    /// arc length, through a polyline's arcs.
    #[test]
    fn divide_and_measure_land_on_a_bulged_polyline_at_equal_arc_length(
        corners in prop::collection::vec((-200.0..200.0_f64, -200.0..200.0_f64, -0.9..0.9_f64), 2..6),
        count in 2..9usize,
    ) {
        let vertices: Vec<Point2> = corners.iter().map(|(x, y, _)| p(*x, *y)).collect();
        prop_assume!(vertices.windows(2).all(|w| w[0].distance_to(w[1]) > 1.0));
        let bulges: Vec<f64> = corners.iter().map(|(_, _, b)| *b).collect();
        let polyline = PolylineEntity::with_bulges(vertices.clone(), bulges.clone(), false);
        let path: Path = polyline.path();
        let length = path.length();
        let tolerance = LinearTolerance::DEFAULT.around(&vertices);
        let mut h = Harness::with_entities([polyline.into()]);
        h.enter_all(&["ZOOM", "E", "DIVIDE"]);
        h.click_world(vertices[0]);
        prop_assert_eq!(h.prompt(), "Enter the number of segments:");
        h.enter(&count.to_string());
        let divided = points(&h);
        prop_assert_eq!(divided.len(), count - 1);
        for (k, at) in divided.iter().enumerate() {
            let location = path.locate(*at, tolerance).expect("on the curve");
            prop_assert!(tolerance.coincident(location.point, *at), "{at:?} off the curve");
            let along = path.length_to(location);
            let want = length * (k + 1) as f64 / count as f64;
            prop_assert!((along - want).abs() < 1e-6 * length.max(1.0), "{along} is not {want}");
        }
        h.undo();
        let step = length / count as f64 * 1.5;
        h.enter("MEASURE");
        h.click_world(vertices[0]);
        h.enter(&format!("{step}"));
        let measured = points(&h);
        prop_assert_eq!(measured.len(), (length / step).floor() as usize);
        for (k, at) in measured.iter().enumerate() {
            let location = path.locate(*at, tolerance).expect("on the curve");
            prop_assert!(tolerance.coincident(location.point, *at));
            let along = path.length_to(location);
            prop_assert!((along - step * (k + 1) as f64).abs() < 1e-6 * length.max(1.0));
        }
    }
}

#[test]
fn an_arc_is_divided_along_its_curve() {
    let mut h =
        Harness::with_entities([cad_document::ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, PI).into()]);
    h.enter_all(&["ZOOM", "E", "DIVIDE"]);
    h.click_world(p(10.0, 0.0));
    h.enter("2");
    let placed = points(&h);
    assert_eq!(placed.len(), 1);
    assert!(
        LinearTolerance::DEFAULT.coincident(placed[0], p(0.0, 10.0)),
        "{placed:?}"
    );
}
