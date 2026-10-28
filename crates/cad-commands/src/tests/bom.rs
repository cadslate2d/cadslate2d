//! BOMTABLE and BALLOON (ADR-0041 §5, #354): the parts drawn as a table and
//! placed as a view; balloons by part number, clear of each other, refused
//! for a part no bill has.

use cad_core::Point2;
use cad_document::Entity;

use crate::bom::BALLOON_GROUP;
use crate::compose::{parts_on, views_on};
use crate::testing::Harness;

fn sheet() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&[
        "UNITS",
        "mm",
        "TITLEBLOCK",
        "A3",
        "Landscape",
        "1",
        "ARMOUR",
        "",
        "",
        "",
        "",
        "Third",
        "0,0",
    ]);
    h
}

#[test]
fn a_bill_of_materials_is_a_table_placed_as_a_view_with_its_parts() {
    let mut h = sheet();
    h.enter_all(&[
        "BOMTABLE",
        "1, Shell, 2, Titanium alloy",
        "2, Frame, 1",
        "1, Again, 1",
    ]);
    assert_eq!(
        h.log().last().unwrap(),
        "Part 1 is in a bill of materials already."
    );
    h.enter_all(&["", ""]);
    assert_eq!(
        h.log().last().unwrap(),
        "A bill of materials of 2 parts placed on TITLE-1 as bom."
    );
    let parts = parts_on(h.session.drawing(), "TITLE-1");
    assert_eq!(parts.iter().map(|p| p.number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(parts[0].material, "Titanium alloy");
    let view = views_on(h.session.drawing(), "TITLE-1")
        .into_iter()
        .find(|v| v.id == "bom")
        .unwrap();
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "BILL OF MATERIALS"))
    );
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "Titanium alloy"))
    );
    // One undo takes the table and its view.
    h.enter("U");
    assert!(views_on(h.session.drawing(), "TITLE-1").is_empty());
    assert!(
        !h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "Titanium alloy"))
    );
    let _ = view;
}

#[test]
fn a_balloon_names_a_part_of_the_bill_and_stays_clear_of_another() {
    let mut h = sheet();
    h.enter_all(&["BOMTABLE", "1, Shell, 2", "", ""]);
    h.enter_all(&["BALLOON", "3"]);
    assert_eq!(
        h.log().last().unwrap(),
        "No bill of materials on the sheet has part 3."
    );
    h.enter_all(&["1", "100,100", "130,130", "1", "105,100", "130,130", ""]);
    let drawing = h.session.drawing();
    let balloons: Vec<_> = drawing
        .groups()
        .iter()
        .filter(|(_, g)| g.name.starts_with(BALLOON_GROUP))
        .map(|(_, g)| g.clone())
        .collect();
    assert_eq!(balloons.len(), 2);
    let centre = |g: &cad_document::Group| {
        let v: serde_json::Value = serde_json::from_str(&g.description).unwrap();
        Point2::new(v["at"][0].as_f64().unwrap(), v["at"][1].as_f64().unwrap())
    };
    assert_eq!(centre(&balloons[0]), Point2::new(130.0, 130.0));
    assert!(
        centre(&balloons[0]).distance_to(centre(&balloons[1])) >= 11.0 - 1e-9,
        "moved clear"
    );
    // A leader, a dot, the circle, the number.
    let first: Vec<&Entity> = balloons[0]
        .members
        .iter()
        .map(|id| drawing.entity(*id).unwrap())
        .collect();
    assert!(matches!(first[0], Entity::Line(_)));
    assert!(
        first
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "1"))
    );
}

#[test]
fn without_a_sheet_or_a_bill_nothing_is_made() {
    let mut h = Harness::new();
    h.enter("BOMTABLE");
    assert_eq!(
        h.log().last().unwrap(),
        "There is no sheet to place a bill of materials on: draw one with TITLEBLOCK first."
    );
    let mut h = sheet();
    h.enter("BALLOON");
    assert_eq!(
        h.log().last().unwrap(),
        "The sheet has no bill of materials: make one with BOMTABLE first."
    );
}

/// NOTES (#355): the lines wrapped to the width in paper millimetres, 3.5
/// high, titled, placed as a view.
#[test]
fn notes_are_a_wrapped_block_placed_as_a_view() {
    let mut h = sheet();
    h.enter_all(&[
        "NOTES",
        "Shell 4 mm titanium alloy.",
        "Joint clearance 15 mm.",
        "",
        "80",
        "",
    ]);
    assert_eq!(h.log().last().unwrap(), "Notes placed on TITLE-1 as notes.");
    let notes = h
        .entities()
        .into_iter()
        .find_map(|e| match e {
            Entity::MText(m) => Some(m),
            _ => None,
        })
        .unwrap();
    assert_eq!(notes.width, 80.0);
    assert_eq!(notes.height, 3.5);
    assert_eq!(
        notes.content,
        "Shell 4 mm titanium alloy.\nJoint clearance 15 mm."
    );
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "NOTES"))
    );
}
