//! Block attributes (#183): ATTDEF, a block that holds one, INSERT asking
//! for its value, ATTEDIT changing it, and a redefinition keeping it.

use cad_core::Point2;
use cad_document::{BlockReferenceEntity, Entity, Properties};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn references(h: &Harness) -> Vec<BlockReferenceEntity> {
    h.entities()
        .iter()
        .filter_map(|entity| match entity {
            Entity::BlockRef(reference) => Some(reference.clone()),
            _ => None,
        })
        .collect()
}

/// The texts a reference shows, placed.
fn shown(h: &Harness, reference: &BlockReferenceEntity) -> Vec<String> {
    h.session
        .drawing()
        .blocks()
        .flatten(reference, Properties::default())
        .into_iter()
        .filter_map(|(entity, _)| match entity {
            Entity::Text(text) => Some(text.content),
            _ => None,
        })
        .collect()
}

/// A tag block: a frame line and a ROOM attribute, defined from the drawing.
fn with_tag_block() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "30,0", ""]);
    h.enter("ATTDEF");
    assert_eq!(h.prompt(), "Enter attribute tag name:");
    h.enter("room");
    assert_eq!(h.prompt(), "Enter attribute prompt <none>:");
    h.enter("Room number");
    assert_eq!(h.prompt(), "Enter default attribute value <none>:");
    h.enter("000");
    assert_eq!(h.prompt(), "Specify start point:");
    h.enter("2,2");
    assert_eq!(h.prompt(), "Specify height <2.5>:");
    h.enter("5");
    assert_eq!(h.prompt(), "Specify rotation angle <0>:");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    h.select_box(p(-10.0, -10.0), p(40.0, 20.0));
    h.enter_all(&["BLOCK", "TAG", "0,0"]);
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    h
}

#[test]
fn attdef_makes_a_definition_drawn_as_its_tag_until_it_is_in_a_block() {
    let mut h = Harness::new();
    h.enter_all(&["ATT", "Part no", "", "", "10,10", "", ""]);
    let entities = h.entities();
    let Entity::AttributeDefinition(definition) = &entities[0] else {
        panic!("{entities:?}");
    };
    assert_eq!(definition.tag, "PART_NO", "one word, upper case");
    assert_eq!(definition.prompt, "");
    assert_eq!(definition.text.content, "");
    assert_eq!(definition.text.insertion, p(10.0, 10.0));
    assert_eq!(definition.text.height, 2.5);
    assert_eq!(definition.as_text().content, "PART_NO");
    assert_eq!(entities[0].kind_name(), "attribute definition");
    // It is picked where its tag is drawn.
    h.click_world(p(14.0, 11.0));
    assert_eq!(h.session.selection().len(), 1);
}

#[test]
fn insert_asks_each_attribute_with_its_prompt_and_default_and_the_value_is_drawn() {
    let mut h = with_tag_block();
    h.enter_all(&["I", "TAG", "100,0", "", ""]);
    assert_eq!(h.prompt(), "Room number <000>:");
    h.enter("101");
    assert_eq!(h.prompt(), "Command:");
    let placed = references(&h);
    assert_eq!(placed.len(), 2, "the one BLOCK made, and this");
    assert_eq!(placed[1].attribute("ROOM"), Some("101"));
    assert_eq!(shown(&h, &placed[1]), vec!["101"]);
    assert_eq!(
        shown(&h, &placed[0]),
        vec!["000"],
        "BLOCK's own shows the default"
    );
    // One undo removes the reference with its values.
    h.undo();
    assert_eq!(references(&h).len(), 1);
    // Enter takes the default.
    h.enter_all(&["I", "TAG", "200,0", "", "", ""]);
    assert_eq!(references(&h)[1].attribute("ROOM"), Some("000"));
    // The summary lists the value beside the reference's rows.
    h.click_world(p(215.0, 2.0));
    let summary = h.session.selection_summary();
    assert!(
        summary
            .geometry
            .iter()
            .any(|(label, value)| label == "ROOM" && value == "000"),
        "{:?}",
        summary.geometry
    );
}

#[test]
fn attedit_changes_a_references_values_in_one_step() {
    let mut h = with_tag_block();
    h.enter_all(&["I", "TAG", "100,0", "", "", "101"]);
    h.enter("ATTEDIT");
    assert_eq!(h.prompt(), "Select block reference:");
    h.click_world(p(115.0, 0.0));
    assert_eq!(h.prompt(), "Room number <101>:");
    h.enter("202");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(references(&h)[1].attribute("ROOM"), Some("202"));
    assert_eq!(shown(&h, &references(&h)[1]), vec!["202"]);
    h.undo();
    assert_eq!(references(&h)[1].attribute("ROOM"), Some("101"));
    // Enter keeps what is there.
    h.enter("ATE");
    h.click_world(p(115.0, 0.0));
    h.enter("");
    assert_eq!(references(&h)[1].attribute("ROOM"), Some("101"));
    // Something else is refused.
    h.enter_all(&["L", "300,300", "310,310", ""]);
    h.enter("ATE");
    h.click_world(p(305.0, 305.0));
    assert_eq!(h.log().last().unwrap(), "That is not a block reference.");
    h.escape();
}

#[test]
fn a_redefinition_keeps_the_values_whose_tags_it_keeps() {
    let mut h = with_tag_block();
    h.enter_all(&["I", "TAG", "100,0", "", "", "101"]);
    // Redefine TAG with the same attribute moved and a new one.
    h.enter_all(&["L", "500,500", "530,500", ""]);
    h.enter_all(&["ATT", "ROOM", "Room number", "000", "510,510", "5", ""]);
    h.enter_all(&["ATT", "FLOOR", "Floor", "1", "510,520", "5", ""]);
    h.select_box(p(490.0, 490.0), p(540.0, 530.0));
    h.enter_all(&["BLOCK", "TAG", "Y", "500,500"]);
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log().last());
    let placed = references(&h);
    let tagged = placed
        .iter()
        .find(|reference| reference.insertion == p(100.0, 0.0))
        .unwrap();
    assert_eq!(tagged.attribute("ROOM"), Some("101"), "kept by tag");
    let texts = shown(&h, tagged);
    assert_eq!(
        texts,
        vec!["101", "1"],
        "the kept value, and the new default"
    );
}

#[test]
fn a_reference_with_attributes_moves_turns_and_mirrors_with_them() {
    let mut h = with_tag_block();
    h.enter_all(&["I", "TAG", "100,0", "", "", "101"]);
    h.select_box(p(90.0, -10.0), p(140.0, 20.0));
    h.enter_all(&["M", "0,0", "0,50"]);
    let moved = references(&h)
        .into_iter()
        .find(|reference| reference.insertion == p(100.0, 50.0))
        .unwrap();
    assert_eq!(moved.attribute("ROOM"), Some("101"));
    assert_eq!(shown(&h, &moved), vec!["101"]);
}
