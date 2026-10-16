//! MLEADER through the session (#240): arrowhead, landing, note; the
//! preview the click commits; the current style; picking, snapping, and
//! grips; one undo.

use cad_core::Point2;
use cad_document::{Entity, MLeaderContent, MLeaderEntity, MLeaderStyle};

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
    assert_eq!(h.prompt(), "Specify leader arrowhead location:");
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
    let layout = mleader.layout(&MLeaderStyle::named("S"));
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
