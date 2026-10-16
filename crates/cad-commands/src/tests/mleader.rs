//! MLEADER through the session (#240): arrowhead, landing, note; the
//! preview the click commits; the current style; picking, snapping, and
//! grips; one undo. Block notes, MLEADEREDIT, and MLEADERSTYLE (#241).

use cad_core::Point2;
use cad_document::{
    Block, BlockId, CircleEntity, Drawing, Entity, EntityRecord, MLeaderContent, MLeaderEntity,
    MLeaderStyle, MLeaderStyleId, Properties, TransactionError,
};

use crate::OverlayItem;
use crate::snap::{SnapMode, entity_snap_points};
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn mleaders(h: &Harness) -> Vec<MLeaderEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::MLeader(mleader) => Some(mleader),
            _ => None,
        })
        .collect()
}

#[test]
fn a_multileader_is_drawn_from_its_arrowhead_to_its_note_as_one_undo() {
    let mut h = Harness::new();
    h.enter("MLD");
    assert_eq!(
        h.prompt(),
        "Specify leader arrowhead location or [Content]:"
    );
    h.enter("0,0");
    h.move_to_world(p(40.0, 30.0));
    let previewed = h.session.overlay().into_iter().any(|item| {
        matches!(item, OverlayItem::PreviewEntity(Entity::MLeader(ref m)) if m.landing == p(40.0, 30.0))
    });
    assert!(previewed, "the leader to the pointer");
    h.enter("40,30");
    assert_eq!(h.prompt(), "Enter the note <none>:");
    h.enter("Drill 6 holes");
    let [mleader] = mleaders(&h).try_into().unwrap();
    assert_eq!(mleader.leaders, vec![vec![p(0.0, 0.0)]]);
    assert_eq!(mleader.landing, p(40.0, 30.0));
    assert_eq!(
        mleader.content,
        MLeaderContent::Text("Drill 6 holes".to_owned())
    );

    // Picked by its note, snapped at its arrowhead, gripped at its points.
    let drawing = h.session.drawing();
    let id = h.ids()[0];
    let entity = drawing.entity(id).unwrap();
    let layout = mleader.layout(&MLeaderStyle::named("S"), cad_document::Tables::none());
    let note = layout.text.unwrap().frame().corners()[0];
    assert!(entity.distance_to(note, drawing.tables()) < 1e-9);
    assert_eq!(
        entity_snap_points(entity, SnapMode::Endpoint, drawing.tables()),
        vec![p(0.0, 0.0)]
    );
    assert_eq!(entity.grips().len(), 2);
    h.undo();
    assert!(mleaders(&h).is_empty(), "one undo");
}

#[test]
fn a_multileader_takes_the_current_style_and_enter_leaves_it_without_a_note() {
    let mut drawing = cad_document::Drawing::new();
    let mut tx = drawing.transaction();
    let style = tx.add_mleader_style(MLeaderStyle::named("Big")).unwrap();
    tx.set_current_mleader_style(style).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["MLEADER", "0,0", "20,10", ""]);
    let [mleader] = mleaders(&h).try_into().unwrap();
    assert_eq!(mleader.style, style);
    assert_eq!(mleader.content, MLeaderContent::None);
}

/// A drawing with a block `TAG`: a circle of radius 2 about its base point.
fn tagged() -> (Drawing, BlockId) {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let tag = tx
        .define_block(Block::new(
            "TAG",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                Entity::from(CircleEntity::new(Point2::ORIGIN, 2.0)),
                Properties::default(),
            )],
        ))
        .unwrap();
    tx.commit();
    (drawing, tag)
}

/// The box a multileader's block note fills.
fn block_frame(h: &Harness, mleader: &MLeaderEntity) -> cad_core::Aabb {
    let drawing = h.session.drawing();
    let tables = drawing.tables();
    let layout = mleader.layout(tables.mleader_style(mleader.style), tables);
    layout.block.expect("a block note").1
}

#[test]
fn a_block_note_is_placed_at_the_dogleg_and_an_unknown_block_is_refused() {
    let (drawing, tag) = tagged();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["MLEADER", "C"]);
    assert_eq!(
        h.prompt(),
        "Enter content type or [Block/Mtext/None] <Mtext>:"
    );
    h.enter_all(&["B", "NOPE"]);
    assert_eq!(h.log().last().unwrap(), "There is no block \"NOPE\".");
    assert_eq!(h.prompt(), "Enter block name:", "asked again");
    h.enter_all(&["tag", "0,0"]);
    h.move_to_world(p(20.0, 10.0));
    let previewed = h.session.overlay().into_iter().any(|item| {
        matches!(item, OverlayItem::PreviewEntity(Entity::MLeader(ref m)) if matches!(m.content, MLeaderContent::Block { .. }))
    });
    assert!(previewed, "the block previewed with the leader");
    h.enter("20,10");
    assert_eq!(h.prompt(), "Command:", "no words asked for");
    let [mleader] = mleaders(&h).try_into().unwrap();
    assert_eq!(
        mleader.content,
        MLeaderContent::Block {
            block: tag,
            scale: 1.0
        }
    );
    // Its left side's middle at the dogleg's end, 5 past the landing.
    let frame = block_frame(&h, &mleader);
    assert!((frame.min().x - 25.0).abs() < 1e-9 && (frame.max().x - 29.0).abs() < 1e-9);
    assert!(((frame.min().y + frame.max().y) / 2.0 - 10.0).abs() < 1e-9);
    // Picked inside its block; the block cannot go while it is a note.
    let drawing = h.session.drawing();
    let entity = drawing.entity(h.ids()[0]).unwrap();
    assert!(entity.distance_to(p(27.0, 10.0), drawing.tables()) < 1e-9);
    let (mut drawing, tag) = tagged();
    let mut tx = drawing.transaction();
    let note = MLeaderContent::Block {
        block: tag,
        scale: 1.0,
    };
    tx.insert(MLeaderEntity::new(p(0.0, 0.0), p(20.0, 10.0), note))
        .unwrap();
    tx.commit();
    let mut tx = drawing.transaction();
    assert_eq!(tx.remove_block(tag), Err(TransactionError::BlockInUse(tag)));
}

#[test]
fn a_style_naming_a_block_makes_block_notes_and_a_change_of_style_redraws_them() {
    let (drawing, tag) = tagged();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["MLEADERSTYLE", "B", "TAG", "SC", "2", ""]);
    let standard = h
        .session
        .drawing()
        .mleader_styles()
        .get(MLeaderStyleId::FIRST)
        .unwrap();
    assert_eq!(
        (standard.block.as_deref(), standard.block_scale),
        (Some("TAG"), 2.0)
    );
    h.enter_all(&["MLEADER", "0,0", "20,10"]);
    let [mleader] = mleaders(&h).try_into().unwrap();
    assert_eq!(
        mleader.content,
        MLeaderContent::Block {
            block: tag,
            scale: 2.0
        }
    );
    assert!((block_frame(&h, &mleader).min().x - 25.0).abs() < 1e-9);

    // A longer dogleg: the note moves with the one row, as one undo.
    let before = h.session.drawing().extents().unwrap();
    h.enter_all(&["MLS", "DO", "10", ""]);
    assert!(
        h.log()
            .contains(&"\"Standard\": dogleg length 10.".to_owned())
    );
    assert!((block_frame(&h, &mleader).min().x - 30.0).abs() < 1e-9);
    let after = h.session.drawing().extents().unwrap();
    assert!(
        (after.max().x - before.max().x - 5.0).abs() < 1e-9,
        "the index follows"
    );
    h.undo();
    assert_eq!(h.session.drawing().extents().unwrap(), before);
    // "." and new notes are words again.
    h.enter_all(&["MLS", "B", ".", ""]);
    h.enter_all(&["MLEADER", "0,0", "-20,10", "Note"]);
    assert_eq!(
        mleaders(&h)[1].content,
        MLeaderContent::Text("Note".to_owned())
    );
}

#[test]
fn mleaderstyle_makes_sets_lists_renames_and_deletes_and_says_why_not() {
    let mut h = Harness::new();
    h.enter("MLEADERSTYLE");
    assert_eq!(
        h.log().last().unwrap(),
        "Current multileader style \"Standard\": arrows 2.5, gap 1, dogleg 5, text 2.5"
    );
    h.enter_all(&["N", "Callout", "A", "3.5", "G", "2", "H", "3"]);
    let current = h.session.drawing().settings().current_mleader_style;
    let callout = h
        .session
        .drawing()
        .mleader_styles()
        .get(current)
        .unwrap()
        .clone();
    assert_eq!(
        callout,
        MLeaderStyle {
            arrow_size: 3.5,
            landing_gap: 2.0,
            text_height: 3.0,
            ..MLeaderStyle::named("Callout")
        }
    );
    h.enter_all(&["H", "0"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The text height must be greater than zero."
    );
    h.enter_all(&["N", "callout"]);
    assert_eq!(
        h.log().last().unwrap(),
        "There is already a multileader style \"Callout\"."
    );
    h.enter("?");
    assert_eq!(
        h.log().last().unwrap(),
        "  \"Callout\": arrows 3.5, gap 2, dogleg 5, text 3, current"
    );
    h.enter_all(&["", "MLEADER", "0,0", "10,10", ""]);
    h.enter_all(&["MLS", "D", "Callout"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Multileader style \"Callout\" is current: set another first."
    );
    h.enter_all(&["S", "Standard", "D", "Callout"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Multileader style \"Callout\" is used by a multileader."
    );
    h.enter_all(&["D", "Standard"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Multileader style \"Standard\" cannot be deleted or renamed."
    );
    h.enter_all(&["R", "Callout", "Tag note"]);
    assert_eq!(
        h.log().last().unwrap(),
        "Multileader style \"Callout\" renamed \"Tag note\"."
    );
    h.enter_all(&["N", "Spare", "S", "Standard", "D", "Spare"]);
    assert_eq!(h.log().last().unwrap(), "1 multileader style deleted.");
    h.enter_all(&["B", "NOPE"]);
    assert_eq!(h.log().last().unwrap(), "There is no block \"NOPE\".");
}

#[test]
fn mleaderedit_adds_and_removes_leaders_keeps_the_last_and_is_one_undo() {
    let mut h = Harness::with_entities([MLeaderEntity::new(
        p(0.0, 0.0),
        p(20.0, 10.0),
        MLeaderContent::Text("A".to_owned()),
    )
    .into()]);
    h.enter("MLEADEREDIT");
    h.click_world(p(10.0, 5.0));
    assert_eq!(
        h.prompt(),
        "Specify leader arrowhead location or [Remove leader]:"
    );
    h.move_to_world(p(0.0, 20.0));
    let previewed = h.session.overlay().into_iter().any(|item| {
        matches!(item, OverlayItem::PreviewEntity(Entity::MLeader(ref m)) if m.leaders.len() == 2)
    });
    assert!(previewed, "the new leader at the pointer");
    h.enter_all(&["0,20", "5,-10"]);
    assert_eq!(mleaders(&h)[0].leaders.len(), 3);
    h.enter_all(&["R", "1,19"]);
    assert_eq!(
        mleaders(&h)[0].leaders,
        vec![vec![p(0.0, 0.0)], vec![p(5.0, -10.0)]],
        "the one nearest the point"
    );
    h.enter_all(&["0,0", "5,-10"]);
    assert_eq!(
        h.log().last().unwrap(),
        "A multileader keeps at least one leader."
    );
    assert_eq!(mleaders(&h)[0].leaders, vec![vec![p(5.0, -10.0)]]);
    h.enter("");
    h.undo();
    assert_eq!(mleaders(&h)[0].leaders, vec![vec![p(0.0, 0.0)]], "one undo");

    // Anything else is not a multileader.
    let mut h =
        Harness::with_entities([cad_document::LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into()]);
    h.enter("MLE");
    h.click_world(p(5.0, 0.0));
    assert_eq!(h.log().last().unwrap(), "A line is not a multileader.");
}
