//! ALIGN (#201): a selection moved by one pair of points, moved and
//! turned by two, scaled too when asked; previewed and committed once.

use std::f64::consts::FRAC_PI_4;

use cad_core::Point2;
use cad_document::{
    Block, BlockReferenceEntity, CircleEntity, Drawing, Entity, EntityRecord, LineEntity,
    Properties, TextEntity,
};
use cad_geometry::Transform;

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// A line from the origin to (20, 0), a circle, a text, and a block
/// reference at the origin, all selected.
fn selected() -> Harness {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let part = tx
        .define_block(Block::new(
            "Part",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                Entity::Line(LineEntity::new(p(0.0, 0.0), p(5.0, 5.0))),
                Properties::default(),
            )],
        ))
        .unwrap();
    tx.insert(LineEntity::new(p(0.0, 0.0), p(20.0, 0.0)))
        .unwrap();
    tx.insert(CircleEntity::new(p(10.0, 5.0), 2.0)).unwrap();
    tx.insert(TextEntity::new(p(0.0, 10.0), 3.0, 0.0, "Ab"))
        .unwrap();
    tx.insert(BlockReferenceEntity::new(part, p(0.0, 0.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["ZOOM", "E"]);
    h.select_box(p(-5.0, -5.0), p(25.0, 15.0));
    assert_eq!(h.session.selection().len(), 4);
    h
}

#[test]
fn one_pair_moves_two_pairs_move_and_turn_and_scale_fits_the_second_pair() {
    let mut h = selected();
    let before = h.entities();
    h.enter("ALIGN");
    assert_eq!(h.prompt(), "Specify first source point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify first destination point:");
    h.enter("100,100");
    assert_eq!(h.prompt(), "Specify second source point <continue>:");
    let dirty = h.enter("");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(last(&h), "4 objects aligned.");
    let moved = Transform::translation(p(100.0, 100.0) - p(0.0, 0.0));
    for (entity, was) in h.entities().iter().zip(&before) {
        assert_eq!(entity, &was.transformed(&moved));
    }
    h.undo();
    assert_eq!(h.entities(), before, "one undo");

    // Two pairs: the line's ends onto a diagonal of the same length.
    h.select_box(p(-5.0, -5.0), p(25.0, 15.0));
    h.enter_all(&["AL", "0,0", "50,50", "20,0", "70,70"]);
    assert_eq!(
        h.prompt(),
        "Scale objects based on alignment points? [Yes/No] <No>:"
    );
    h.enter("");
    assert!(h.lines()[0].start.distance_to(p(50.0, 50.0)) < 1e-9);
    let end = h.lines()[0].end;
    assert!(
        end.distance_to(p(
            50.0 + 20.0 / 2.0_f64.sqrt(),
            50.0 + 20.0 / 2.0_f64.sqrt()
        )) < 1e-9,
        "turned, not scaled: {end:?}"
    );
    let Entity::BlockRef(reference) = &h.entities()[3] else {
        panic!("a reference");
    };
    assert!((reference.rotation - FRAC_PI_4).abs() < 1e-9);
    assert_eq!(reference.scale, 1.0);
    let Entity::Circle(circle) = &h.entities()[1] else {
        panic!("a circle");
    };
    assert_eq!(circle.radius, 2.0);
    h.undo();

    // Scaled: the second source point lands on the second destination.
    h.select_box(p(-5.0, -5.0), p(25.0, 15.0));
    h.enter_all(&["AL", "0,0", "50,50", "20,0", "70,70", "Y"]);
    assert!(h.lines()[0].start.distance_to(p(50.0, 50.0)) < 1e-9);
    assert!(h.lines()[0].end.distance_to(p(70.0, 70.0)) < 1e-9);
    let Entity::Circle(circle) = &h.entities()[1] else {
        panic!("a circle");
    };
    assert!((circle.radius - 2.0 * 2.0_f64.sqrt()).abs() < 1e-9);
    let Entity::Text(text) = &h.entities()[2] else {
        panic!("a text");
    };
    assert!((text.height - 3.0 * 2.0_f64.sqrt()).abs() < 1e-9);
    assert!((text.rotation - FRAC_PI_4).abs() < 1e-9);
    h.undo();
    assert_eq!(h.entities(), before);
}

#[test]
fn align_previews_from_the_points_so_far_touches_nothing_and_asks_for_objects() {
    let mut h = selected();
    let ids = h.ids();
    h.enter_all(&["ALIGN", "0,0"]);
    let revision = h.session.drawing().revision();
    h.move_to_world(p(30.0, 40.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::PreviewTransformed {
                ids: ids.clone(),
                transform: Transform::translation(p(30.0, 40.0) - p(0.0, 0.0)),
            })
    );
    h.enter_all(&["30,40", "20,0"]);
    h.move_to_world(p(30.0, 60.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::PreviewTransformed {
                ids,
                transform: Transform::aligning(
                    p(0.0, 0.0),
                    p(20.0, 0.0),
                    p(30.0, 40.0),
                    p(30.0, 60.0),
                    false
                )
                .unwrap(),
            })
    );
    assert_eq!(h.session.drawing().revision(), revision);
    h.escape();
    assert_eq!(h.session.drawing().revision(), revision);
    // Points that coincide are refused.
    h.select_box(p(-5.0, -5.0), p(25.0, 15.0));
    h.enter_all(&["AL", "0,0", "1,1", "0,0"]);
    assert_eq!(
        last(&h),
        "The second source point must differ from the first."
    );
    h.escape();
    h.escape();
    assert!(h.session.selection().is_empty());
    // Nothing selected: asked.
    h.enter("ALIGN");
    assert!(h.prompt().starts_with("Select objects"));
    h.enter("");
    assert_eq!(last(&h), "No objects selected.");
}
