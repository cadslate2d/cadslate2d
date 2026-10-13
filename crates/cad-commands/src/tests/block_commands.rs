//! BLOCK, INSERT, and EXPLODE through the session (#148): each command,
//! each refusal, and one undo each.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_document::{
    Block, BlockId, BlockReferenceEntity, Color, Drawing, Entity, EntityRecord, Layer, LayerId,
    LineEntity, PolylineEntity, Properties,
};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line(LineEntity::new(p(x0, y0), p(x1, y1)))
}

/// Two lines making an L at the origin.
fn corner() -> Harness {
    Harness::with_entities([line(0.0, 0.0, 10.0, 0.0), line(0.0, 0.0, 0.0, 10.0)])
}

/// A drawing with a chair block and one reference to it at (100, 0).
fn chair_drawing() -> (Drawing, BlockId) {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let chair = tx
        .define_block(Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![
                EntityRecord::new(line(0.0, 0.0, 10.0, 0.0), Properties::default()),
                EntityRecord::new(line(0.0, 0.0, 0.0, 10.0), Properties::default()),
            ],
        ))
        .unwrap();
    tx.insert(BlockReferenceEntity::new(chair, p(100.0, 0.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    (drawing, chair)
}

fn with_chair() -> Harness {
    Harness::with_drawing(chair_drawing().0)
}

fn references(h: &Harness) -> Vec<BlockReferenceEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::BlockRef(reference) => Some(reference),
            _ => None,
        })
        .collect()
}

// ── BLOCK ──

#[test]
fn block_replaces_the_selection_with_one_reference_and_one_undo_restores_it() {
    let mut h = corner();
    h.enter("B");
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter("Chair");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("0,0");
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(5.0, 0.0));
    h.click_world(p(0.0, 5.0));
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.log().last().unwrap(),
        "Block \"Chair\" defined from 2 objects."
    );

    let drawing = h.session.drawing();
    let chair = drawing.blocks().find("Chair").expect("a block");
    assert_eq!(drawing.blocks().get(chair).unwrap().entities.len(), 2);
    assert_eq!(
        references(&h),
        vec![BlockReferenceEntity::new(chair, p(0.0, 0.0), 1.0, 0.0)]
    );
    assert_eq!(h.entities().len(), 1, "the lines are gone");
    assert!(h.session.selection().is_empty());
    // The reference draws where the lines were.
    h.click_world(p(5.0, 0.0));
    assert_eq!(h.session.selection().len(), 1);
    h.escape();

    h.undo();
    assert_eq!(h.lines().len(), 2, "one undo brings the lines back");
    assert!(h.session.drawing().blocks().find("Chair").is_none());
    h.redo();
    assert_eq!(references(&h).len(), 1);
}

#[test]
fn block_takes_a_selection_made_beforehand_and_a_name_with_a_space() {
    let mut h = corner();
    h.select_box(p(-20.0, -20.0), p(20.0, 20.0));
    assert_eq!(h.session.selection().len(), 2);
    h.enter_all(&["B", "Arm chair", "5,5"]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.log().last().unwrap(),
        "Block \"Arm chair\" defined from 2 objects."
    );
    let drawing = h.session.drawing();
    let block = drawing
        .blocks()
        .find("arm CHAIR")
        .expect("names ignore case");
    assert_eq!(drawing.blocks().get(block).unwrap().base, p(5.0, 5.0));
    assert_eq!(references(&h)[0].insertion, p(5.0, 5.0));
}

#[test]
fn block_keeps_the_objects_properties_inside_the_definition() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let walls = tx.add_layer(Layer::named("walls")).unwrap();
    tx.insert_with(
        line(0.0, 0.0, 10.0, 0.0),
        Properties {
            color: Color::Index(1),
            ..Properties::on(walls)
        },
    )
    .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.click_world(p(5.0, 0.0));
    h.enter_all(&["B", "Wall", "0,0"]);
    let drawing = h.session.drawing();
    let wall = drawing.blocks().find("Wall").unwrap();
    let record = &drawing.blocks().get(wall).unwrap().entities[0];
    assert_eq!(record.properties.layer, walls);
    assert_eq!(record.properties.color, Color::Index(1));
    // The reference itself is a new object on the current layer.
    assert_eq!(h.properties()[0].layer, LayerId::FIRST);
}

#[test]
fn block_refuses_bad_names_an_empty_selection_and_enter_at_the_name() {
    let mut h = corner();
    h.enter_all(&["B", "Ch/air"]);
    assert_eq!(
        h.log().last().unwrap(),
        "A block name cannot contain \"/\"."
    );
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter_all(&["Chair", "0,0", ""]);
    assert_eq!(h.log().last().unwrap(), "No objects selected.");
    assert_eq!(h.prompt(), "Command:");
    assert!(h.session.drawing().blocks().find("Chair").is_none());
    assert_eq!(h.lines().len(), 2);

    h.enter_all(&["B", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn an_existing_name_asks_before_redefining_and_every_reference_follows() {
    let (mut drawing, chair) = chair_drawing();
    let mut tx = drawing.transaction();
    tx.insert(BlockReferenceEntity::new(chair, p(200.0, 0.0), 2.0, 0.0))
        .unwrap();
    tx.insert(line(0.0, 0.0, 0.0, 30.0)).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);

    h.enter_all(&["B", "Chair"]);
    assert_eq!(
        h.prompt(),
        "Block \"Chair\" already exists. Redefine it? [Yes/No] <No>:"
    );
    // Enter is No: another name, then.
    h.enter("");
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter_all(&["Chair", "Yes", "0,0"]);
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(0.0, 15.0));
    h.enter("");
    assert_eq!(
        h.log().last().unwrap(),
        "Block \"Chair\" redefined from 1 object."
    );

    // The old references are tall stools now: 30 at (100, 0), 60 at (200, 0).
    let drawing = h.session.drawing();
    assert_eq!(drawing.blocks().get(chair).unwrap().entities.len(), 1);
    let bounds: Vec<Point2> = references(&h)
        .iter()
        .map(|reference| drawing.tables().bounds_of(reference).max())
        .collect();
    assert_eq!(bounds, vec![p(100.0, 30.0), p(200.0, 60.0), p(0.0, 30.0)]);

    h.undo();
    let drawing = h.session.drawing();
    assert_eq!(drawing.blocks().get(chair).unwrap().entities.len(), 2);
    assert_eq!(
        h.lines().len(),
        1,
        "one undo: the line is back, the stool gone"
    );
}

#[test]
fn a_block_cannot_contain_itself() {
    let mut h = with_chair();
    let revision = h.session.drawing().revision();
    // Redefining Chair from its own reference.
    h.click_world(p(105.0, 0.0));
    h.enter_all(&["B", "Chair", "Yes", "100,0"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Block \"Chair\" cannot contain itself."
    );
    assert_eq!(h.session.drawing().revision(), revision, "nothing changed");
    assert_eq!(h.prompt(), "Command:");
}

// ── INSERT ──

#[test]
fn insert_places_a_reference_with_its_scale_and_rotation_and_one_undo_removes_it() {
    let mut h = with_chair();
    h.enter("I");
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter("chair");
    assert_eq!(h.prompt(), "Specify insertion point:");
    h.enter("50,50");
    assert_eq!(h.prompt(), "Specify scale factor <1>:");
    h.enter("2");
    assert_eq!(h.prompt(), "Specify rotation angle <0>:");
    h.enter("90");
    assert_eq!(h.prompt(), "Command:");

    let placed = references(&h);
    assert_eq!(placed.len(), 2);
    let chair = h.session.drawing().blocks().find("Chair").unwrap();
    assert_eq!(placed[1].block, chair);
    assert_eq!(placed[1].insertion, p(50.0, 50.0));
    assert_eq!(placed[1].scale, 2.0);
    assert!((placed[1].rotation - FRAC_PI_2).abs() < 1e-12);
    assert!(!placed[1].mirrored);
    // Its geometry is where a click finds it: the seat, 20 long, now points up.
    h.click_world(p(50.0, 60.0));
    assert_eq!(h.session.selection().len(), 1);
    h.escape();

    h.undo();
    assert_eq!(references(&h).len(), 1);
}

#[test]
fn insert_takes_enter_for_its_defaults_and_lists_blocks() {
    let mut h = with_chair();
    h.enter_all(&["I", "?"]);
    assert_eq!(h.log().last().unwrap(), "Blocks: Chair.");
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter_all(&["Chair", "0,0", "", ""]);
    let placed = references(&h);
    assert_eq!(
        placed[1],
        BlockReferenceEntity::new(placed[0].block, p(0.0, 0.0), 1.0, 0.0)
    );
}

#[test]
fn insert_refuses_an_unknown_name_and_a_scale_that_is_not_one() {
    let mut h = with_chair();
    h.enter_all(&["I", "Table"]);
    assert_eq!(h.log().last().unwrap(), "There is no block \"Table\".");
    assert_eq!(h.prompt(), "Enter block name:");
    h.enter_all(&["Chair", "0,0", "0"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The scale factor must be greater than zero."
    );
    assert_eq!(h.prompt(), "Specify scale factor <1>:");
    // A factor is a plain number: a length is refused.
    h.enter("2mm");
    assert_eq!(h.prompt(), "Specify scale factor <1>:");
    h.enter("-1");
    assert_eq!(h.prompt(), "Specify scale factor <1>:");
    assert_eq!(references(&h).len(), 1, "nothing placed yet");
    h.escape();
    assert_eq!(references(&h).len(), 1);

    let mut empty = Harness::new();
    empty.enter_all(&["I", "?"]);
    assert_eq!(
        empty.log().last().unwrap(),
        "There are no blocks in this drawing."
    );
}

#[test]
fn insert_previews_the_reference_at_the_pointer_without_touching_the_drawing() {
    let mut h = with_chair();
    let revision = h.session.drawing().revision();
    h.enter_all(&["I", "Chair"]);
    h.move_to_world(p(30.0, 40.0));
    let previewed: Vec<Entity> = h
        .session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewEntity(entity) => Some(entity),
            _ => None,
        })
        .collect();
    let chair = h.session.drawing().blocks().find("Chair").unwrap();
    assert_eq!(
        previewed,
        vec![Entity::BlockRef(BlockReferenceEntity::new(
            chair,
            p(30.0, 40.0),
            1.0,
            0.0
        ))]
    );
    // At the scale prompt the pointer's distance from the insertion point
    // is the scale; at the rotation prompt its direction is the angle.
    h.enter("0,0");
    h.move_to_world(p(3.0, 4.0));
    let scaled = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::BlockRef(reference)) => Some(reference),
        _ => None,
    });
    assert_eq!(scaled.map(|r| r.scale), Some(5.0));
    h.enter("1");
    h.move_to_world(p(0.0, 7.0));
    let turned = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::BlockRef(reference)) => Some(reference),
        _ => None,
    });
    assert!((turned.unwrap().rotation - FRAC_PI_2).abs() < 1e-12);
    assert_eq!(h.session.drawing().revision(), revision);
    // A click at the rotation prompt takes that angle.
    h.click_world(p(0.0, 7.0));
    let placed = references(&h);
    assert!((placed[1].rotation - FRAC_PI_2).abs() < 1e-12);
    assert_eq!(placed[1].scale, 1.0);
}

// ── EXPLODE ──

#[test]
fn explode_turns_a_reference_into_its_parts_as_they_were_drawn_and_one_undo_restores_it() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let walls = tx.add_layer(Layer::named("walls")).unwrap();
    let chair = tx
        .define_block(Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![
                // Layer 0 and ByBlock: drawn as the reference is.
                EntityRecord::new(
                    line(0.0, 0.0, 10.0, 0.0),
                    Properties {
                        color: Color::ByBlock,
                        ..Properties::default()
                    },
                ),
                // Its own layer and colour: kept.
                EntityRecord::new(
                    line(0.0, 0.0, 0.0, 10.0),
                    Properties {
                        color: Color::Index(5),
                        ..Properties::on(walls)
                    },
                ),
            ],
        ))
        .unwrap();
    tx.insert_with(
        BlockReferenceEntity::new(chair, p(100.0, 0.0), 2.0, 0.0),
        Properties {
            color: Color::Index(1),
            ..Properties::on(walls)
        },
    )
    .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);

    h.enter("X");
    assert_eq!(
        h.prompt(),
        "Select objects or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(110.0, 0.0));
    h.enter("");
    assert_eq!(
        h.log().last().unwrap(),
        "1 block reference exploded into 2 objects."
    );
    assert_eq!(
        h.lines(),
        vec![
            LineEntity::new(p(100.0, 0.0), p(120.0, 0.0)),
            LineEntity::new(p(100.0, 0.0), p(100.0, 20.0)),
        ]
    );
    let properties = h.properties();
    assert_eq!(
        (properties[0].layer, properties[0].color),
        (walls, Color::Index(1))
    );
    assert_eq!(
        (properties[1].layer, properties[1].color),
        (walls, Color::Index(5))
    );
    assert!(h.session.selection().is_empty());
    assert!(
        h.session.drawing().blocks().find("Chair").is_some(),
        "the definition stays"
    );

    h.undo();
    assert_eq!(references(&h).len(), 1, "one undo: the reference is back");
    assert!(h.lines().is_empty());
}

#[test]
fn explode_takes_a_nest_apart_one_level_at_a_time() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let chair = tx
        .define_block(Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                line(0.0, 0.0, 10.0, 0.0),
                Properties::default(),
            )],
        ))
        .unwrap();
    let pair = tx
        .define_block(Block::new(
            "Pair",
            Point2::ORIGIN,
            vec![
                EntityRecord::new(
                    BlockReferenceEntity::new(chair, Point2::ORIGIN, 1.0, 0.0),
                    Properties::default(),
                ),
                EntityRecord::new(
                    BlockReferenceEntity::new(chair, p(20.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                ),
            ],
        ))
        .unwrap();
    tx.insert(BlockReferenceEntity::new(pair, p(100.0, 0.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);

    // Selected first, then the command: no prompt at all.
    h.click_world(p(125.0, 0.0));
    h.enter("X");
    assert_eq!(h.prompt(), "Command:");
    let chairs = references(&h);
    assert_eq!(chairs.len(), 2);
    assert!(chairs.iter().all(|reference| reference.block == chair));
    assert_eq!(chairs[1].insertion, p(120.0, 0.0));

    h.select_box(p(90.0, -5.0), p(140.0, 5.0));
    h.enter("X");
    assert_eq!(
        h.log().last().unwrap(),
        "2 block references exploded into 2 objects."
    );
    assert_eq!(h.lines().len(), 2);
    assert!(references(&h).is_empty());
}

#[test]
fn explode_turns_a_polyline_into_lines_and_arcs_and_refuses_the_rest_by_name() {
    let mut h = Harness::with_entities([
        Entity::Polyline(PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)],
            vec![0.0, 1.0],
            false,
        )),
        line(50.0, 0.0, 60.0, 0.0),
    ]);
    h.select_box(p(-20.0, -20.0), p(70.0, 20.0));
    h.enter("X");
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        [
            "1 polyline exploded into 2 objects.",
            "A line cannot be exploded."
        ]
    );
    let kinds: Vec<&str> = h.entities().iter().map(Entity::kind_name).collect();
    assert_eq!(kinds, ["line", "line", "arc"]);

    h.undo();
    let kinds: Vec<&str> = h.entities().iter().map(Entity::kind_name).collect();
    assert_eq!(kinds, ["polyline", "line"]);

    // Undo re-selected what it brought back (DESIGN §19.7); drop that.
    h.escape();

    // Nothing to explode at all: said once per kind, and no transaction.
    let revision = h.session.drawing().revision();
    h.click_world(p(55.0, 0.0));
    h.enter("X");
    assert_eq!(h.log().last().unwrap(), "A line cannot be exploded.");
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter_all(&["X", ""]);
    assert_eq!(h.log().last().unwrap(), "No objects selected.");
}
