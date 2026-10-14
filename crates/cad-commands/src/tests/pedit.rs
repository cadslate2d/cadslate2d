//! PEDIT and DONUT (#204): a polyline edited as one, a width drawn as a
//! band and kept through edits, rings drawn as wide polylines.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn polyline(h: &Harness, index: usize) -> PolylineEntity {
    match &h.entities()[index] {
        Entity::Polyline(polyline) => polyline.clone(),
        other => panic!("not a polyline: {other:?}"),
    }
}

/// An open polyline with a half-turn bulge in its middle segment.
fn bulged() -> Entity {
    PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(40.0, 0.0), p(40.0, 20.0), p(0.0, 20.0)],
        vec![0.0, 1.0, 0.0, 0.0],
        false,
    )
    .into()
}

#[test]
fn pedit_closes_opens_widens_decurves_and_undoes_within_the_command_as_one_undo() {
    let mut h = Harness::with_entities([bulged()]);
    h.enter_all(&["ZOOM", "E", "PEDIT"]);
    assert_eq!(h.prompt(), "Select polyline:");
    h.click_world(p(20.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Enter an option or [Close/Open/Join/Width/Edit vertex/Decurve/Undo]:"
    );
    h.enter("C");
    assert!(polyline(&h, 0).closed);
    h.enter("C");
    assert_eq!(last(&h), "The polyline is already closed.");
    h.enter("O");
    assert!(!polyline(&h, 0).closed);
    h.enter("W");
    assert_eq!(h.prompt(), "Specify new width for all segments:");
    h.enter("-1");
    assert_eq!(last(&h), "The width must be zero or a positive number.");
    let dirty = h.enter("2");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(polyline(&h, 0).width(), 2.0);
    assert!(!polyline(&h, 0).band().is_empty(), "drawn as a band");
    h.enter("D");
    assert!(!polyline(&h, 0).has_arcs());
    assert_eq!(polyline(&h, 0).width(), 2.0, "the width stays");
    h.enter("U");
    assert!(polyline(&h, 0).has_arcs(), "the decurve undone");
    h.enter_all(&["U", "U", "U"]);
    assert_eq!(polyline(&h, 0).width(), 0.0);
    assert!(!polyline(&h, 0).closed);
    h.enter("U");
    assert_eq!(last(&h), "Nothing to undo.");
    h.enter_all(&["W", "3", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(polyline(&h, 0).width(), 3.0);
    h.undo();
    assert_eq!(h.entities()[0], bulged(), "one undo for the whole command");
}

#[test]
fn pedit_joins_what_meets_the_polylines_ends_and_keeps_its_width() {
    let mut h = Harness::with_entities([
        PolylineEntity::new(vec![p(0.0, 0.0), p(40.0, 0.0)], false)
            .with_width(2.0)
            .into(),
        LineEntity::new(p(40.0, 0.0), p(40.0, 30.0)).into(),
        ArcEntity::new(p(60.0, 30.0), 20.0, std::f64::consts::PI, 3.0 * FRAC_PI_2).into(),
        LineEntity::new(p(100.0, 100.0), p(120.0, 100.0)).into(),
    ]);
    h.enter_all(&["ZOOM", "E", "PE"]);
    h.click_world(p(20.0, 0.0));
    h.enter("J");
    assert!(h.prompt().starts_with("Select objects to join"));
    h.select_box(p(-10.0, -10.0), p(130.0, 110.0));
    h.enter("");
    assert_eq!(last(&h), "2 objects added to the polyline.");
    assert_eq!(h.session.drawing().shown().count(), 2, "the far line stays");
    let joined = polyline(&h, 0);
    assert_eq!(joined.vertices.len(), 4);
    assert_eq!(joined.width(), 2.0);
    assert!(joined.has_arcs());
    h.enter("J");
    h.select_box(p(90.0, 90.0), p(130.0, 110.0));
    h.enter("");
    assert_eq!(
        last(&h),
        "Nothing joins to the polyline: no selected object meets its ends."
    );
    h.enter("");
    h.undo();
    assert_eq!(h.session.drawing().shown().count(), 4);
}

#[test]
fn pedit_edits_vertices_next_previous_insert_move_and_straighten() {
    let mut h = Harness::with_entities([bulged()]);
    h.enter_all(&["ZOOM", "E", "PEDIT"]);
    h.click_world(p(20.0, 0.0));
    h.enter("E");
    assert_eq!(
        h.prompt(),
        "Enter a vertex editing option or [Next/Previous/Insert/Move/Straighten/Exit] <Next>:"
    );
    h.move_to_world(p(5.0, 5.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::HotGrip { at: p(0.0, 0.0) })
    );
    h.enter("");
    h.move_to_world(p(5.0, 5.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::HotGrip { at: p(40.0, 0.0) })
    );
    h.enter("S");
    assert!(
        !polyline(&h, 0).has_arcs(),
        "the arc from the marked vertex straightened"
    );
    h.enter("S");
    assert_eq!(last(&h), "The segment from the marked vertex is straight.");
    h.enter("I");
    assert_eq!(h.prompt(), "Specify location of new vertex:");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(50.0, 10.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Polyline(preview)) if preview.vertices.len() == 5
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter("50,10");
    assert_eq!(polyline(&h, 0).vertices[2], p(50.0, 10.0));
    assert_eq!(polyline(&h, 0).vertices.len(), 5);
    h.move_to_world(p(5.0, 5.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::HotGrip { at: p(50.0, 10.0) }),
        "the new vertex is marked"
    );
    h.enter("M");
    assert_eq!(h.prompt(), "Specify new location for marked vertex:");
    h.enter("55,15");
    assert_eq!(polyline(&h, 0).vertices[2], p(55.0, 15.0));
    h.enter("P");
    h.move_to_world(p(5.0, 5.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::HotGrip { at: p(40.0, 0.0) })
    );
    h.enter_all(&["P", "P"]);
    h.move_to_world(p(5.0, 5.0));
    assert!(
        h.session
            .overlay()
            .contains(&OverlayItem::HotGrip { at: p(0.0, 20.0) }),
        "round the end"
    );
    h.enter("E");
    assert_eq!(
        h.prompt(),
        "Enter an option or [Close/Open/Join/Width/Edit vertex/Decurve/Undo]:"
    );
    h.enter("");
    h.undo();
    assert_eq!(h.entities()[0], bulged());
}

#[test]
fn a_line_or_an_arc_is_offered_as_a_polyline_and_the_rest_refused() {
    let mut h = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(40.0, 0.0)).into(),
        ArcEntity::new(p(100.0, 0.0), 20.0, 0.0, FRAC_PI_2).into(),
        CircleEntity::new(p(200.0, 0.0), 10.0).into(),
    ]);
    h.enter_all(&["ZOOM", "E", "PEDIT"]);
    h.click_world(p(210.0, 0.0));
    assert_eq!(last(&h), "A circle cannot be edited as a polyline.");
    h.click_world(p(20.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Object selected is not a polyline. Turn it into one? [Yes/No] <Yes>:"
    );
    h.enter("N");
    assert_eq!(h.prompt(), "Select polyline:");
    assert!(matches!(h.entities()[0], Entity::Line(_)));
    h.click_world(p(20.0, 0.0));
    h.enter("");
    assert_eq!(
        h.prompt(),
        "Enter an option or [Close/Open/Join/Width/Edit vertex/Decurve/Undo]:"
    );
    let converted = polyline(&h, 0);
    assert_eq!(converted.vertices, vec![p(0.0, 0.0), p(40.0, 0.0)]);
    h.enter("");
    h.enter("PE");
    h.click_world(p(120.0, 0.0));
    h.enter("Y");
    let arc = polyline(&h, 1);
    assert_eq!(arc.vertices.len(), 2);
    assert!((arc.bulge(0) - 1.0_f64.sqrt().powi(0) * (FRAC_PI_2 / 4.0).tan()).abs() < 1e-12);
    h.enter("");
}

#[test]
fn a_wide_polyline_is_picked_on_its_band_and_keeps_its_width_through_edits() {
    let mut h = Harness::with_entities([PolylineEntity::new(
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
        false,
    )
    .with_width(4.0)
    .into()]);
    h.enter_all(&["ZOOM", "E"]);
    // Picked on the band, 1.5 off the centre line, where a plain polyline
    // would need the aperture.
    h.click_world(p(50.0, 1.5));
    assert_eq!(h.session.selection().len(), 1);
    assert_eq!(
        h.session.selection_summary().geometry.last().unwrap().1,
        "4.0000"
    );
    h.enter_all(&["SC", "0,0", "2"]);
    assert_eq!(polyline(&h, 0).width(), 8.0, "scaled with it");
    h.click_world(p(100.0, 1.5));
    h.enter_all(&["OFFSET", "10"]);
    h.click_world(p(100.0, 1.5));
    h.click_world(p(100.0, -30.0));
    h.enter("");
    assert_eq!(polyline(&h, 1).width(), 8.0, "an offset keeps it");
    h.enter_all(&["BREAK"]);
    h.click_world(p(50.0, 1.5));
    h.enter_all(&["F", "40,0", "60,0"]);
    assert_eq!(polyline(&h, 0).width(), 8.0, "a piece keeps it");
}

#[test]
fn donut_draws_rings_until_enter_and_a_disc_for_zero_inside() {
    let mut h = Harness::new();
    h.enter("DONUT");
    assert_eq!(h.prompt(), "Specify inside diameter of donut <0.5>:");
    h.enter("-1");
    assert_eq!(
        last(&h),
        "The inside diameter must be zero or a positive number."
    );
    h.enter("");
    assert_eq!(h.prompt(), "Specify outside diameter of donut <1>:");
    h.enter("0.25");
    assert_eq!(
        last(&h),
        "The outside diameter must be larger than the inside."
    );
    h.enter("2");
    assert_eq!(h.prompt(), "Specify center of donut <exit>:");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(30.0, 30.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewEntity(Entity::Polyline(ring)) if ring.closed && ring.width() == 0.75
    )));
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter_all(&["10,10", "20,20", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.entities().len(), 2);
    let ring = polyline(&h, 0);
    assert!(ring.closed);
    assert_eq!(ring.width(), 0.75);
    assert_eq!(
        ring.vertices,
        vec![p(10.0 - 0.625, 10.0), p(10.0 + 0.625, 10.0)]
    );
    assert_eq!((ring.bulge(0), ring.bulge(1)), (1.0, 1.0));
    h.enter_all(&["DO", "0", "4", "0,0", ""]);
    let disc = polyline(&h, 2);
    assert_eq!(disc.width(), 2.0);
    assert_eq!(disc.vertices, vec![p(-1.0, 0.0), p(1.0, 0.0)]);
    h.undo();
    assert_eq!(h.entities().len(), 2, "one undo per command");
}
