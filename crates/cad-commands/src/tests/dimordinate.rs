//! DIMORDINATE and QDIM through the session (#227): the axis from the
//! drag or forced, the datum moved, each of QDIM's rows, the preview the
//! click commits, and one undo each.

use cad_core::Point2;
use cad_document::{DimensionEntity, Entity, Measure};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn dimensions(h: &Harness) -> Vec<DimensionEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .collect()
}

/// Each ordinate's axis — true for an X — and value.
fn ordinates(h: &Harness) -> Vec<(bool, f64)> {
    dimensions(h)
        .iter()
        .filter_map(|dimension| match dimension.measure {
            Measure::Ordinate { x, .. } => Some((x, dimension.value())),
            _ => None,
        })
        .collect()
}

#[test]
fn an_ordinate_takes_its_axis_from_the_drag_or_as_told_and_its_datum_moves() {
    let mut h = Harness::new();
    h.enter_all(&["DOR"]);
    assert_eq!(h.prompt(), "Specify feature location or [Datum]:");
    h.enter("80,50");
    assert_eq!(
        h.prompt(),
        "Specify leader endpoint or [Xdatum/Ydatum/Text/Angle]:"
    );
    // The preview is the dimension the click makes.
    h.move_to_world(p(80.0, 5.0));
    let previewed: Vec<Entity> = h
        .session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewEntity(entity) => Some(entity),
            _ => None,
        })
        .collect();
    h.enter("80,5");
    assert_eq!(ordinates(&h), [(true, 80.0)], "dragged down: the X");
    assert_eq!(previewed, h.entities());

    h.enter_all(&["DIMORDINATE", "D"]);
    assert_eq!(h.prompt(), "Specify datum point <0,0>:");
    h.enter("50,20");
    assert!(said(&h, "Ordinates are measured from 50,20."));
    h.enter_all(&["125,60", "45,63"]);
    assert_eq!(ordinates(&h)[1], (false, 40.0), "dragged across: the Y");
    // The datum is kept for the session; Xdatum forces the axis.
    h.enter_all(&["DOR", "125,60", "X", "45,63"]);
    assert_eq!(ordinates(&h)[2], (true, 75.0));
    h.enter_all(&["DOR", "125,60", "T", "HOLE", "130,5"]);
    assert_eq!(dimensions(&h)[3].text_override.as_deref(), Some("HOLE"));
    h.undo();
    assert_eq!(dimensions(&h).len(), 3, "one undo");
}

/// A plate 100 by 60 with a hole at (30, 30), selected whole.
fn plate() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["PLINE", "0,0", "100,0", "100,60", "0,60", "C"]);
    h.enter_all(&["CIRCLE", "30,30", "8"]);
    h.select_box(p(-20.0, -20.0), p(120.0, 80.0));
    h
}

/// Each linear dimension's points' coordinates along X, as pairs.
fn spans(h: &Harness) -> Vec<(f64, f64, f64)> {
    dimensions(h)
        .iter()
        .filter_map(|dimension| match dimension.measure {
            Measure::Linear { a, b, .. } => Some((a.x, b.x, dimension.line_point.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn qdim_dimensions_a_row_continuous_baseline_or_ordinate_as_one_undo() {
    let mut h = plate();
    h.enter("QDIM");
    assert_eq!(
        h.prompt(),
        "Specify dimension line position or [Continuous/Baseline/Ordinate/Datum point] <Continuous>:"
    );
    h.enter("50,-20");
    assert!(said(&h, "2 dimensions made."));
    assert_eq!(spans(&h), [(0.0, 30.0, -20.0), (30.0, 100.0, -20.0)]);
    h.undo();
    assert!(dimensions(&h).is_empty(), "one undo");

    // Baseline: from the first, each a spacing further out.
    h.select_box(p(-20.0, -20.0), p(120.0, 80.0));
    h.enter_all(&["QDIM", "B", "50,-20"]);
    assert_eq!(spans(&h), [(0.0, 30.0, -20.0), (0.0, 100.0, -23.75)]);

    // Ordinate, beside the plate: vertical, the Y of each from the datum.
    h.select_box(p(-20.0, -20.0), p(120.0, 80.0));
    h.enter_all(&["QDIM", "O", "D", "0,0", "-20,30"]);
    assert_eq!(ordinates(&h), [(false, 0.0), (false, 30.0), (false, 60.0)]);
}

#[test]
fn qdim_asks_for_something_to_dimension() {
    let mut h = Harness::new();
    h.enter_all(&["TEXT", "0,0", "5", "0", "Note"]);
    h.select_box(p(-20.0, -20.0), p(120.0, 80.0));
    h.enter("QDIM");
    assert!(said(
        &h,
        "Nothing selected has ends or centres to dimension."
    ));
    h.enter_all(&["QDIM", ""]);
    assert!(said(&h, "No objects selected."));
}
