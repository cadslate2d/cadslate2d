//! Block references through the session (#147): picked as one object,
//! selected by window and crossing, snapped into through nesting, and
//! hidden with their layer. The model is tested in `cad-document`.

use cad_core::Point2;
use cad_document::{
    Block, BlockId, BlockReferenceEntity, Drawing, Entity, EntityRecord, LineEntity, Properties,
};

use crate::testing::Harness;
use crate::{OverlayItem, SnapMode};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line(LineEntity::new(p(x0, y0), p(x1, y1)))
}

/// A chair (an L of two lines), a pair of chairs 20 apart, and the pair
/// placed at (100, 0) and, twice the size, at (100, 100).
fn nested() -> (Harness, BlockId, BlockId) {
    let (drawing, chair, pair) = nested_drawing();
    (Harness::with_drawing(drawing), chair, pair)
}

fn nested_drawing() -> (Drawing, BlockId, BlockId) {
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
    tx.insert(BlockReferenceEntity::new(pair, p(100.0, 100.0), 2.0, 0.0))
        .unwrap();
    tx.commit();
    (drawing, chair, pair)
}

#[test]
fn a_click_anywhere_on_a_references_geometry_selects_the_whole_reference() {
    let (mut h, ..) = nested();
    let ids = h.ids();
    // On the second chair's upright of the first pair: one object.
    h.click_world(p(120.0, 5.0));
    assert_eq!(
        h.session.selection().iter().collect::<Vec<_>>(),
        vec![ids[0]]
    );
    assert_eq!(
        h.session.ui_state().selection.headline(),
        "1 block reference"
    );
    assert_eq!(
        h.session.ui_state().selection.geometry[0],
        ("Block".to_owned(), "Pair".to_owned())
    );
    // Above the chairs is empty space, even inside the reference's box.
    h.escape();
    h.click_world(p(115.0, 30.0));
    assert!(h.session.selection().is_empty());
    // (That click began a selection box; Escape drops it.)
    h.escape();
    // The scaled pair is picked along its scaled lines.
    h.click_world(p(160.0, 100.0));
    assert_eq!(
        h.session.selection().iter().collect::<Vec<_>>(),
        vec![ids[1]]
    );
}

#[test]
fn window_and_crossing_selection_treat_a_reference_as_one_object() {
    let (mut h, ..) = nested();
    let ids = h.ids();
    // A window round the first pair takes it; the second is only crossed.
    h.select_box(p(90.0, -10.0), p(140.0, 110.0));
    assert_eq!(
        h.session.selection().iter().collect::<Vec<_>>(),
        vec![ids[0]]
    );
    h.escape();
    // A crossing box over both takes both. Its first corner is off the
    // geometry, or it would pick instead of starting a box.
    h.select_box(p(150.0, 110.0), p(90.0, -10.0));
    assert_eq!(h.session.selection().len(), 2);
}

#[test]
fn object_snap_reaches_inside_nested_references_and_offers_the_insertion() {
    let (mut h, ..) = nested();
    h.enter("L");
    // The far end of the second chair's seat in the scaled pair: the chair
    // sits 40 along (20, scaled) and its seat is 20 long: (160, 100).
    h.move_to_world(p(159.0, 101.0));
    let snap = h.session.snap().expect("a snap");
    assert_eq!(
        (snap.mode, snap.point),
        (SnapMode::Endpoint, p(160.0, 100.0))
    );
    // The midpoint of the first chair's upright in the first pair.
    h.move_to_world(p(101.0, 5.5));
    let snap = h.session.snap().expect("a snap");
    assert_eq!((snap.mode, snap.point), (SnapMode::Midpoint, p(100.0, 5.0)));
    // The insertion point itself, where the first chair's corner also is:
    // Endpoint ranks first, as it would on any corner.
    h.move_to_world(p(100.5, 100.5));
    let snap = h.session.snap().expect("a snap");
    assert_eq!(snap.point, p(100.0, 100.0));
    assert!(matches!(
        snap.mode,
        SnapMode::Endpoint | SnapMode::Insertion
    ));
    h.escape();
}

#[test]
fn a_reference_moves_as_one_and_is_previewed_whole() {
    let (mut h, ..) = nested();
    let ids = h.ids();
    h.click_world(p(120.0, 5.0));
    h.enter_all(&["M", "100,0", "200,0"]);
    let moved = h.session.drawing().entity(ids[0]).cloned().unwrap();
    let Entity::BlockRef(reference) = moved else {
        panic!("still a reference");
    };
    assert_eq!(reference.insertion, p(200.0, 0.0));
    // Its preview while moving was the whole pair.
    h.escape();
    h.click_world(p(220.0, 5.0));
    h.enter_all(&["M", "0,0"]);
    h.move_to_world(p(0.0, 10.0));
    let previewed: Vec<usize> = h
        .session
        .overlay()
        .iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewTransformed { ids, .. } => Some(ids.len()),
            _ => None,
        })
        .collect();
    assert_eq!(
        previewed,
        vec![1],
        "one preview of one reference, drawn whole"
    );
    h.escape();
}

#[test]
fn after_a_redefinition_every_copy_is_picked_where_its_new_geometry_is() {
    let (mut drawing, chair, _) = nested_drawing();
    // The chair becomes a tall stool: both pairs show stools, no seats.
    let mut tx = drawing.transaction();
    tx.redefine_block(
        chair,
        Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                line(0.0, 0.0, 0.0, 30.0),
                Properties::default(),
            )],
        ),
    )
    .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.click_world(p(100.0, 25.0));
    assert_eq!(
        h.session.selection().len(),
        1,
        "the taller stool is where the pair is"
    );
    h.escape();
    h.click_world(p(110.0, -15.0));
    assert!(h.session.selection().is_empty(), "the seat is gone");
    h.escape();
    h.click_world(p(140.0, 150.0));
    assert_eq!(
        h.session.selection().len(),
        1,
        "the scaled pair's second stool, 60 tall"
    );
}

#[test]
fn curve_commands_refuse_a_reference_by_name() {
    let (mut h, ..) = nested();
    h.enter_all(&["O", "3"]);
    h.click_world(p(120.0, 5.0));
    h.click_world(p(130.0, 30.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A block reference cannot be offset."
    );
    h.escape();
    h.enter("BR");
    h.click_world(p(120.0, 5.0));
    h.click_world(p(120.0, 8.0));
    assert_eq!(
        h.log().last().unwrap(),
        "A block reference cannot be broken."
    );
}

#[test]
fn a_references_lines_are_cutting_edges_and_boundaries() {
    let (drawing, ..) = nested_drawing();
    let mut h = Harness::with_drawing(drawing);
    // A line through the first pair's second chair upright at x = 120.
    h.enter_all(&["L", "115,5", "135,5", ""]);
    h.enter("TR");
    h.click_world(p(120.0, 2.0));
    h.enter("");
    h.click_world(p(117.0, 5.0));
    h.escape();
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(120.0, 5.0), p(135.0, 5.0))],
        "the reference's upright cut the line"
    );
    // And back out to it with EXTEND.
    h.enter_all(&["EX", ""]);
    h.click_world(p(133.0, 5.0));
    h.escape();
    assert_eq!(
        h.lines(),
        vec![LineEntity::new(p(120.0, 5.0), p(135.0, 5.0))],
        "nothing lies beyond the far end"
    );
    h.enter_all(&["L", "150,105", "170,105", ""]);
    h.enter_all(&["EX", ""]);
    h.click_world(p(152.0, 105.0));
    h.escape();
    assert_eq!(
        h.lines()[1],
        LineEntity::new(p(140.0, 105.0), p(170.0, 105.0)),
        "extended to the scaled pair's upright at x = 140"
    );
}
