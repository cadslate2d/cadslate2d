//! The clipboard through the session (#223): copied and pasted within a
//! drawing, about a base point, cut as one undo, pasted after OPEN with
//! its rows brought in by name, and the keys.

use cad_core::Point2;
use cad_document::{
    Block, BlockReferenceEntity, CircleEntity, Color, Drawing, Entity, EntityRecord, Layer,
    LineEntity, Properties,
};

use crate::testing::Harness;
use crate::{KeyAction, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

/// Two lines from (0, 0), 100 long, selected — picked well away from
/// their grips.
fn two_lines_selected() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", "", "L", "0,0", "0,100", ""]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 50.0));
    assert_eq!(h.session.selection().len(), 2);
    h
}

#[test]
fn copied_objects_paste_where_the_point_puts_their_lower_left() {
    let mut h = two_lines_selected();
    h.enter("COPYCLIP");
    assert!(said(&h, "2 objects copied to the clipboard."));
    assert!(h.session.selection().is_empty());
    h.enter("PASTECLIP");
    assert_eq!(h.prompt(), "Specify insertion point:");
    h.move_to_world(p(100.0, 100.0));
    let outline = h
        .session
        .overlay()
        .into_iter()
        .filter(|item| matches!(item, OverlayItem::PreviewSegment { .. }))
        .count();
    assert_eq!(outline, 4, "the box the paste covers");
    h.enter("100,100");
    assert!(said(&h, "2 objects pasted."));
    let lines = h.lines();
    assert_eq!(lines[2], LineEntity::new(p(100.0, 100.0), p(200.0, 100.0)));
    assert_eq!(lines[3], LineEntity::new(p(100.0, 100.0), p(100.0, 200.0)));
    // A paste is one undo; the clipboard keeps what it holds.
    h.undo();
    assert_eq!(h.lines().len(), 2);
    h.enter_all(&["PASTECLIP", "-50,0"]);
    assert_eq!(h.lines().len(), 4);
}

#[test]
fn copybase_asks_for_the_base_and_cutclip_is_one_undo() {
    let mut h = two_lines_selected();
    h.enter("COPYBASE");
    assert_eq!(h.prompt(), "Specify base point:");
    h.enter("100,0");
    h.enter_all(&["PASTECLIP", "50,50"]);
    assert_eq!(h.lines()[2], LineEntity::new(p(-50.0, 50.0), p(50.0, 50.0)));

    let mut h = two_lines_selected();
    h.enter("CUTCLIP");
    assert!(said(&h, "2 objects cut to the clipboard."));
    assert!(h.lines().is_empty());
    h.undo();
    assert_eq!(h.lines().len(), 2, "the cut undone");
    h.enter_all(&["PASTECLIP", "0,0"]);
    assert_eq!(h.lines().len(), 4, "and the clipboard still holds them");
}

#[test]
fn a_paste_after_open_brings_its_rows_in_by_name() {
    let mut source = Drawing::new();
    let mut tx = source.transaction();
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
                Properties::on(walls),
            )],
        ))
        .unwrap();
    tx.insert_with(
        LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)),
        Properties::on(walls),
    )
    .unwrap();
    tx.insert(BlockReferenceEntity::new(seat, p(20.0, 0.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(source);
    h.enter_all(&["SELECT", "ALL", ""]);
    h.session.key(KeyAction::Copy);
    assert!(said(&h, "2 objects copied to the clipboard."));

    // Another drawing, with its own `WALLS`, blue.
    let mut other = Drawing::new();
    let mut tx = other.transaction();
    let blue = tx
        .add_layer(Layer {
            color: Color::Index(5),
            ..Layer::named("WALLS")
        })
        .unwrap();
    tx.commit();
    h.session.open_drawing(other, "other.dxf", vec![], vec![]);
    h.session.key(KeyAction::Paste);
    assert_eq!(h.prompt(), "Specify insertion point:");
    h.enter("100,0");
    let drawing = h.session.drawing();
    assert_eq!(drawing.layers().get(blue).unwrap().color, Color::Index(5));
    let pasted: Vec<_> = drawing
        .records()
        .map(|(_, record)| record.clone())
        .collect();
    assert_eq!(pasted.len(), 2);
    assert_eq!(
        pasted[0].properties.layer, blue,
        "the drawing's row by that name"
    );
    let Entity::BlockRef(reference) = &*pasted[1].entity else {
        panic!("a reference")
    };
    assert_eq!(drawing.blocks().get(reference.block).unwrap().name, "seat");
    // The base was the lower left of what was copied: the seat's circle
    // reaches 2 below the line.
    assert_eq!(reference.insertion, p(120.0, 2.0));
}

#[test]
fn an_empty_clipboard_says_so_and_the_keys_wait_for_the_idle_prompt() {
    let mut h = Harness::new();
    h.session.key(KeyAction::Paste);
    assert!(said(&h, "The clipboard is empty."));
    h.enter("L");
    h.session.key(KeyAction::Copy);
    assert_eq!(h.prompt(), "Specify first point:", "the line goes on");
}
