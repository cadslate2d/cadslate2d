//! INSERT of a file and WBLOCK through the session (#222), with the host's
//! part played by the test: a file picked is a drawing handed back; a file
//! written is a drawing taken.

use cad_core::Point2;
use cad_document::{
    Block, BlockReferenceEntity, CircleEntity, Color, Drawing, Entity, EntityRecord, Layer,
    LineEntity, Properties,
};

use crate::HostRequest;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

/// A drawing as a file would bring it: a red layer `walls` with a line on
/// it, and a reference to a block `seat` holding a circle.
fn file() -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let walls = tx
        .add_layer(Layer {
            color: Color::Index(1),
            ..Layer::named("walls")
        })
        .unwrap();
    let seat = tx
        .define_block(Block::new(
            "seat",
            p(0.0, 0.0),
            vec![EntityRecord::new(
                CircleEntity::new(p(0.0, 0.0), 2.0),
                Properties::default(),
            )],
        ))
        .unwrap();
    tx.insert_with(
        LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)),
        Properties::on(walls),
    )
    .unwrap();
    tx.insert(BlockReferenceEntity::new(seat, p(5.0, 5.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    drawing
}

/// The references in the drawing, by their block's name.
fn placed(h: &Harness) -> Vec<String> {
    let drawing = h.session.drawing();
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::BlockRef(reference) => {
                Some(drawing.blocks().get(reference.block)?.name.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn insert_takes_a_file_as_a_block_with_its_rows_and_places_it() {
    let mut h = Harness::new();
    // The drawing has its own `WALLS`, blue.
    h.enter_all(&["LA", "N", "WALLS", "C", "5", "WALLS", ""]);
    h.enter("INSERT");
    assert_eq!(h.prompt(), "Enter block name, or ~ for a file:");
    h.enter("~");
    assert_eq!(h.session.take_host_request(), Some(HostRequest::InsertFile));
    assert_eq!(h.prompt(), "Choose the file to insert:");
    h.session.insert_drawing(file(), "Chair.dxf", vec![]);
    assert!(said(&h, "Block \"Chair\" made from Chair.dxf: 2 objects."));
    assert_eq!(h.prompt(), "Specify insertion point:");
    h.enter_all(&["100,0", "", ""]);
    assert_eq!(placed(&h), ["Chair"]);

    let drawing = h.session.drawing();
    let chair = drawing
        .blocks()
        .get(drawing.blocks().find("Chair").unwrap())
        .unwrap();
    let walls = drawing.layers().find("walls").unwrap();
    assert_eq!(chair.entities[0].properties.layer, walls);
    assert_eq!(
        drawing.layers().get(walls).unwrap().color,
        Color::Index(5),
        "the drawing's row by that name"
    );
    assert!(
        drawing.blocks().find("seat").is_some(),
        "the block it places came too"
    );

    // Inserted again: the same block, placed again.
    h.enter_all(&["INSERT", "~"]);
    h.session
        .insert_drawing(file(), "chair.dxf", vec!["1 thing dropped.".into()]);
    assert!(said(&h, "1 thing dropped."));
    assert!(said(&h, "Block \"chair\" is already defined: placing it."));
    h.enter_all(&["200,0", "", ""]);
    assert_eq!(placed(&h), ["Chair", "Chair"]);
    // One undo takes the second placing, the next the block and all.
    h.undo();
    h.undo();
    assert!(h.session.drawing().blocks().find("Chair").is_none());

    // No file picked: the command gives up.
    h.enter_all(&["INSERT", "~", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn wblock_writes_a_block_about_its_base_and_inserting_it_brings_it_back() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let doors = tx.add_layer(Layer::named("doors")).unwrap();
    let block = Block::new(
        "door",
        p(5.0, 0.0),
        vec![EntityRecord::new(
            LineEntity::new(p(5.0, 0.0), p(5.0, 20.0)),
            Properties::on(doors),
        )],
    );
    tx.define_block(block).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.enter("WBLOCK");
    assert_eq!(h.prompt(), "Enter name of block to write <the selection>:");
    h.enter("door");
    assert_eq!(h.prompt(), "Enter file name <door>:");
    h.enter("");
    assert!(said(&h, "Writing door.dxf: 1 object."));
    assert_eq!(h.session.take_host_request(), Some(HostRequest::WriteBlock));
    let (name, written) = h.session.take_block_file().unwrap();
    assert_eq!(name, "door.dxf");
    assert!(h.session.take_block_file().is_none(), "taken once");
    let records: Vec<_> = written
        .records()
        .map(|(_, record)| record.clone())
        .collect();
    assert_eq!(
        *records[0].entity,
        Entity::Line(LineEntity::new(p(0.0, 0.0), p(0.0, 20.0))),
        "the base at the file's origin"
    );
    assert_eq!(
        written
            .layers()
            .get(records[0].properties.layer)
            .unwrap()
            .name,
        "doors"
    );

    // Inserted into another drawing, it is the block again.
    let mut other = Harness::new();
    other.enter_all(&["INSERT", "~"]);
    other.session.insert_drawing(written, &name, vec![]);
    other.enter_all(&["0,0", "", ""]);
    let target = other.session.drawing();
    let door = target
        .blocks()
        .get(target.blocks().find("door").unwrap())
        .unwrap();
    assert_eq!(*door.entities[0].entity, *records[0].entity);
}

#[test]
fn wblock_writes_the_selection_about_a_base_point_and_leaves_it() {
    let mut h = Harness::new();
    h.enter_all(&["L", "10,10", "30,10", "", "L", "10,10", "10,40", ""]);
    h.click_world(p(20.0, 10.0));
    h.click_world(p(10.0, 25.0));
    h.enter("W");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter_all(&["10,10", "parts"]);
    assert!(said(&h, "Writing parts.dxf: 2 objects."));
    let (name, written) = h.session.take_block_file().unwrap();
    assert_eq!(name, "parts.dxf");
    assert_eq!(written.records().count(), 2);
    assert_eq!(h.lines().len(), 2, "the objects stay");
    assert!(h.session.selection().is_empty());
}
