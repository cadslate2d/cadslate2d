//! LAYMRG and LAYDEL through the session (#236): objects moved with their
//! other properties, objects in a block, a locked layer, the refusals,
//! and one undo each.

use cad_core::Point2;
use cad_document::{Color, Entity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn has_layer(h: &Harness, name: &str) -> bool {
    h.session.drawing().layers().find(name).is_some()
}

/// Lines on A (one red), B, and C, and a block made of a line on B.
fn layered() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LAYER", "N", "A,B,C,KEEP", "S", "A", ""]);
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter_all(&[
        "COLOR", "red", "L", "0,10", "100,10", "", "COLOR", "bylayer",
    ]);
    h.enter_all(&["LAYER", "S", "B", ""]);
    h.enter_all(&["L", "0,20", "100,20", ""]);
    h.enter_all(&["L", "0,200", "50,200", ""]);
    h.enter_all(&["BLOCK", "Part", "0,200"]);
    h.click_world(p(25.0, 200.0));
    h.enter("");
    h.enter_all(&["LAYER", "S", "C", ""]);
    h.enter_all(&["L", "0,30", "100,30", ""]);
    h.enter_all(&["LAYER", "S", "KEEP", ""]);
    h
}

#[test]
fn layers_merged_carry_their_objects_and_blocks_onto_the_target() {
    let mut h = layered();
    h.enter_all(&["LAYER", "LOCK", "A", ""]);
    h.enter("LAYMRG");
    assert_eq!(h.prompt(), "Select object on layer to merge or [Name]:");
    h.click_world(p(50.0, 0.0));
    assert!(said(&h, "Selected layers: \"A\"."), "a locked layer too");
    h.enter_all(&["N", "B"]);
    assert!(said(&h, "Selected layers: \"A\", \"B\"."));
    h.enter("");
    assert_eq!(h.prompt(), "Select object on target layer or [Name]:");
    h.click_world(p(50.0, 30.0));
    // Two lines on A; on B a line, the block's reference, and the line
    // inside the block.
    assert!(said(&h, "2 layers merged into \"C\": 5 objects moved."));
    assert!(!has_layer(&h, "A") && !has_layer(&h, "B"));
    let drawing = h.session.drawing();
    let c = drawing.layers().find("C").unwrap();
    assert!(
        drawing
            .records()
            .all(|(_, record)| record.properties.layer == c
                || drawing.layers().get(record.properties.layer).unwrap().name == "KEEP")
    );
    let red = drawing
        .records()
        .filter(|(_, record)| record.properties.color == Color::Index(1))
        .count();
    assert_eq!(red, 1, "its colour kept");
    let part = drawing
        .blocks()
        .get(drawing.blocks().find("Part").unwrap())
        .unwrap();
    assert!(
        part.entities
            .iter()
            .all(|record| record.properties.layer == c)
    );
    h.undo();
    assert!(has_layer(&h, "A") && has_layer(&h, "B"), "one undo");
}

#[test]
fn layers_deleted_take_what_is_on_them_after_a_confirmation() {
    let mut h = layered();
    let before = h.entities().len();
    h.enter_all(&["LAYDEL", "N", "C", ""]);
    assert_eq!(
        h.prompt(),
        "Delete the layers and everything on them? [Yes/No] <No>:"
    );
    h.enter("");
    assert!(said(&h, "Nothing deleted."));
    h.enter_all(&["LAYDEL", "N", "C,A", "", "Y"]);
    assert!(said(&h, "2 layers deleted, with 3 objects on them."));
    assert_eq!(h.entities().len(), before - 3);
    assert!(
        h.entities()
            .iter()
            .all(|entity| matches!(entity, Entity::Line(_) | Entity::BlockRef(_)))
    );
    h.undo();
    assert_eq!(h.entities().len(), before);

    // Layer 0, the current layer, and a layer into itself are refused.
    h.enter_all(&["LAYDEL", "N", "0", "", "Y"]);
    assert!(said(&h, "Layer \"0\" cannot be renamed or deleted."));
    h.enter_all(&["LAYDEL", "N", "KEEP", "", "Y"]);
    assert!(said(
        &h,
        "Layer \"KEEP\" is the current layer: it cannot be switched off, frozen, or deleted."
    ));
    h.enter_all(&["LAYMRG", "N", "A", "", "N", "A"]);
    assert!(said(&h, "Layer \"A\" cannot be merged into itself."));
}
