//! TITLEBLOCK (#340): a sheet's border and title block in the model at a
//! scale, in the drawing's unit, and on a layout in paper millimetres, one
//! undo, its fields kept in its group.

use cad_core::Point2;
use cad_document::Entity;

use crate::testing::Harness;

fn texts(h: &Harness) -> Vec<String> {
    h.entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Text(t) => Some(t.content),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sheet_in_the_model_is_drawn_at_its_scale_in_the_drawings_unit() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "m"]);
    h.enter("TITLEBLOCK");
    assert_eq!(h.prompt(), "Paper size? [A4/A3/A2/A1/A0] <A3>:");
    h.enter_all(&["A3", ""]);
    assert_eq!(h.prompt(), "Specify the scale, 1:N <1>:");
    h.enter_all(&[
        "50",
        "HOUSE PLAN",
        "A-101",
        "L. Araujo",
        "2026-10-27",
        "",
        "",
        "10,20",
    ]);
    assert_eq!(
        h.log().last().unwrap(),
        "A3 landscape sheet drawn as TITLE-1, at 1:50."
    );
    // The sheet's edge: 420 × 297 mm at 1:50 is 21 × 14.85 m.
    let Some(Entity::Polyline(edge)) = h.entities().into_iter().next() else {
        panic!()
    };
    assert!(edge.vertices[0].distance_to(Point2::new(10.0, 20.0)) < 1e-9);
    assert!(edge.vertices[2].distance_to(Point2::new(31.0, 34.85)) < 1e-9);
    let said = texts(&h);
    for field in [
        "HOUSE PLAN",
        "A-101",
        "L. Araujo",
        "2026-10-27",
        "1:50",
        "1/1",
        "1ST ANGLE",
    ] {
        assert!(said.contains(&field.to_owned()), "{field}: {said:?}");
    }
    let drawing = h.session.drawing();
    let group = drawing
        .groups()
        .iter()
        .map(|(_, g)| g)
        .find(|g| g.name == "TITLE-1")
        .unwrap();
    assert!(
        group.description.contains("\"title\":\"HOUSE PLAN\""),
        "{}",
        group.description
    );
    h.undo();
    assert!(h.entities().is_empty());
}

#[test]
fn a_sheet_on_a_layout_is_in_paper_millimetres_and_asks_no_scale() {
    let mut h = Harness::new();
    h.enter_all(&["LAYOUT", "N", "Sheet1"]);
    h.enter_all(&["TITLEBLOCK", "A4", "P"]);
    assert_eq!(h.prompt(), "Enter the title:");
    h.enter_all(&["BRACKET", "", "", "", "", "T", ""]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("A4 portrait sheet drawn as TITLE-1"),
        "{:?}",
        h.log()
    );
    let Some(Entity::Polyline(edge)) = h.entities().into_iter().next() else {
        panic!()
    };
    assert!(edge.vertices[2].distance_to(Point2::new(210.0, 297.0)) < 1e-9);
    assert!(texts(&h).contains(&"3RD ANGLE".to_owned()));
}
