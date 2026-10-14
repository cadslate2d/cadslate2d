//! FIND through the session (#216): every kind of text found and replaced
//! in one transaction, the options, a locked layer's text found but kept,
//! and what is not there.

use cad_core::Point2;
use cad_document::{
    AttributeDefinitionEntity, Block, BlockReferenceEntity, DimensionEntity, Drawing, Entity,
    EntityRecord, Layer, LeaderEntity, LineEntity, MTextEntity, Measure, Properties, TextEntity,
};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

/// Every text the drawing's entities hold, in order.
fn texts(h: &Harness) -> Vec<String> {
    h.entities()
        .into_iter()
        .flat_map(|entity| match entity {
            Entity::Text(text) => vec![text.content],
            Entity::MText(text) => vec![text.content],
            Entity::AttributeDefinition(definition) => vec![definition.text.content],
            Entity::BlockRef(reference) => reference
                .attributes
                .into_iter()
                .map(|attribute| attribute.value)
                .collect(),
            Entity::Leader(leader) => leader.text.into_iter().collect(),
            Entity::Dimension(dimension) => dimension.text_override.into_iter().collect(),
            _ => vec![],
        })
        .collect()
}

/// One of every kind of text, each with "door" in it, and a line; the
/// last text on a layer `locked`, which is locked.
fn every_kind() -> Harness {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let locked = tx
        .add_layer(Layer {
            locked: true,
            ..Layer::named("locked")
        })
        .unwrap();
    let block = tx
        .define_block(Block::new(
            "TAG",
            p(0.0, 0.0),
            vec![EntityRecord::new(
                AttributeDefinitionEntity::new(
                    "MARK",
                    "Mark",
                    TextEntity::new(p(0.0, 0.0), 2.5, 0.0, "x"),
                ),
                Properties::default(),
            )],
        ))
        .unwrap();
    tx.insert(TextEntity::new(p(0.0, 0.0), 2.5, 0.0, "Door A"))
        .unwrap();
    tx.insert(MTextEntity::new(
        p(0.0, 20.0),
        0.0,
        2.5,
        0.0,
        "Door B\nthe door",
    ))
    .unwrap();
    tx.insert(AttributeDefinitionEntity::new(
        "DOOR",
        "Which door?",
        TextEntity::new(p(0.0, 40.0), 2.5, 0.0, "door"),
    ))
    .unwrap();
    tx.insert(
        BlockReferenceEntity::new(block, p(50.0, 0.0), 1.0, 0.0).with_attribute("MARK", "Door C"),
    )
    .unwrap();
    tx.insert(
        LeaderEntity::new(vec![p(80.0, 0.0), p(90.0, 10.0)]).saying(Some("door here".into())),
    )
    .unwrap();
    tx.insert(
        DimensionEntity::new(
            Measure::Aligned {
                a: p(0.0, -20.0),
                b: p(40.0, -20.0),
            },
            p(20.0, -30.0),
        )
        .saying(Some("door width".into())),
    )
    .unwrap();
    tx.insert(LineEntity::new(p(0.0, -50.0), p(40.0, -50.0)))
        .unwrap();
    tx.insert_with(
        TextEntity::new(p(0.0, 60.0), 2.5, 0.0, "locked door"),
        Properties::on(locked),
    )
    .unwrap();
    tx.commit();
    Harness::with_drawing(drawing)
}

#[test]
fn find_selects_what_holds_the_words_in_every_kind_of_text() {
    let mut h = every_kind();
    h.enter("FIND");
    assert_eq!(h.prompt(), "Text to find or [Match case/Whole words]:");
    h.enter("door");
    assert_eq!(h.prompt(), "Replace with <find only>:");
    h.enter("");
    assert!(said(&h, "Found 8 times in 7 objects."));
    // The locked text is found, but nothing on a locked layer is selected.
    assert_eq!(h.session.selection().len(), 6);
    h.escape();
    h.enter_all(&["FIND", "window", ""]);
    assert!(said(&h, "\"window\" is not found."));
}

#[test]
fn find_replaces_in_one_transaction_and_keeps_what_a_locked_layer_holds() {
    let mut h = every_kind();
    let before = texts(&h);
    h.enter_all(&["FIND", "door", "gate"]);
    assert!(said(&h, "Replaced 7 times in 6 objects."));
    assert!(said(&h, "1 object on a locked layer was left as it was."));
    assert_eq!(
        texts(&h),
        [
            "gate A",
            "gate B\nthe gate",
            "gate",
            "gate C",
            "gate here",
            "gate width",
            "locked door"
        ]
    );
    h.undo();
    assert_eq!(texts(&h), before, "one undo");
}

#[test]
fn match_case_and_whole_words_narrow_what_is_replaced() {
    let mut h = every_kind();
    h.enter_all(&["FIND", "M"]);
    assert!(said(&h, "Case must match."));
    h.enter_all(&["Door", "Gate"]);
    assert!(said(&h, "Replaced 3 times in 3 objects."));
    assert_eq!(texts(&h)[1], "Gate B\nthe door");

    h.enter_all(&["FIND", "W", "the", ""]);
    assert!(said(&h, "Whole words only."));
    assert!(said(&h, "Found 1 time in 1 object."));
    // Not whole words: "the" in "there" counts too.
    h.enter_all(&["FIND", "here", "there"]);
    h.enter_all(&["FIND", "the", ""]);
    assert!(said(&h, "Found 2 times in 2 objects."));
}
