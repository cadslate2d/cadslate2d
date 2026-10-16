//! DRAWORDER, TEXTTOFRONT, and WIPEOUT through the session (#226): each
//! option, the tiers they leave, one undo each, and what they refuse.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, WipeoutEntity};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn order(h: &Harness, id: EntityId) -> i16 {
    h.session
        .drawing()
        .record(id)
        .map(|record| record.order)
        .unwrap()
}

fn orders(h: &Harness) -> Vec<i16> {
    h.ids().into_iter().map(|id| order(h, id)).collect()
}

/// Three horizontal lines, at y = 0, 20, and 40.
fn three_lines() -> Harness {
    let mut h = Harness::new();
    for y in [0, 20, 40] {
        h.enter_all(&["L", &format!("0,{y}"), &format!("100,{y}"), ""]);
    }
    h
}

#[test]
fn front_and_back_move_the_selection_past_everything_and_undo_as_one() {
    let mut h = three_lines();
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("DR");
    assert_eq!(
        h.prompt(),
        "Enter object ordering option or [Above objects/Under objects/Front/Back] <Back>:"
    );
    h.enter("F");
    assert!(said(&h, "2 objects brought to the front."));
    assert_eq!(orders(&h), [1, 1, 0]);
    // Already there: nothing changes, and nothing is recorded.
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter_all(&["DRAWORDER", "F"]);
    assert!(said(&h, "The objects are already in front."));
    h.undo();
    assert_eq!(orders(&h), [0, 0, 0], "one undo");

    // Back, by default, with the objects asked for.
    h.enter("DRAWORDER");
    assert!(h.prompt().starts_with("Select objects or ["));
    h.click_world(p(50.0, 40.0));
    h.enter_all(&["", ""]);
    assert!(said(&h, "1 object sent to the back."));
    assert_eq!(orders(&h), [0, 0, -1]);
    // A new object goes on top.
    h.enter_all(&["L", "0,60", "100,60", ""]);
    assert_eq!(orders(&h), [0, 0, -1, 0]);
}

#[test]
fn above_and_under_go_next_to_the_reference_objects() {
    let mut h = three_lines();
    let ids = h.ids();
    h.click_world(p(50.0, 40.0));
    h.enter_all(&["DR", "F"]);
    h.click_world(p(50.0, 20.0));
    h.enter_all(&["DR", "F"]);
    assert_eq!(orders(&h), [0, 2, 1]);
    // The first line just above the third, beneath the second.
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["DR", "A"]);
    assert!(h.prompt().starts_with("Select reference objects or ["));
    h.click_world(p(50.0, 40.0));
    h.enter("");
    assert!(said(&h, "1 object brought above."));
    assert_eq!(order(&h, ids[0]), 2);
    assert_eq!(order(&h, ids[2]), 1);
    assert_eq!(
        order(&h, ids[1]),
        3,
        "what was over the reference made room"
    );
    // Under the third.
    h.click_world(p(50.0, 20.0));
    h.enter_all(&["DR", "U"]);
    h.click_world(p(50.0, 40.0));
    h.enter("");
    assert!(order(&h, ids[1]) < order(&h, ids[2]));
    // No reference: nothing moves.
    h.click_world(p(50.0, 20.0));
    h.enter_all(&["DR", "U", ""]);
    assert!(said(&h, "No reference objects selected."));
}

#[test]
fn texttofront_brings_every_text_or_dimension_in_front() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter_all(&["TEXT", "10,10", "5", "0", "Note"]);
    h.enter_all(&["DIMLINEAR", "0,0", "100,0", "50,-20"]);
    h.enter_all(&["L", "0,50", "100,50", ""]);
    let ids = h.ids();
    h.enter("TEXTTOFRONT");
    assert_eq!(
        h.prompt(),
        "Bring to front or [Text/Dimensions/Leaders/All] <All>:"
    );
    h.enter("T");
    assert!(said(&h, "1 object brought to the front."));
    assert_eq!(order(&h, ids[1]), 1);
    assert_eq!(order(&h, ids[2]), 0);
    // Both kinds, each keeping its place against the other.
    h.enter_all(&["TEXTTOFRONT", ""]);
    assert!(said(&h, "2 objects brought to the front."));
    assert_eq!((order(&h, ids[2]), order(&h, ids[1])), (1, 2));
    h.enter_all(&["TEXTTOFRONT", "L"]);
    assert!(said(
        &h,
        "There is nothing of that kind to bring to the front."
    ));
    h.undo();
    assert_eq!(order(&h, ids[2]), 0);
}

fn wipeouts(h: &Harness) -> Vec<WipeoutEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Wipeout(wipeout) => Some(wipeout),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wipeout_is_made_over_everything_and_what_follows_lies_over_it() {
    let mut h = three_lines();
    h.enter("WIPEOUT");
    assert_eq!(
        h.prompt(),
        "Specify first point or [Frames/Polyline] <Polyline>:"
    );
    h.enter_all(&["10,-10", "60,-10"]);
    h.enter("");
    assert!(said(&h, "A wipeout needs three points at least."));
    // The preview closes the ring at the pointer.
    h.move_to_world(p(60.0, 30.0));
    let closing = h.session.overlay().into_iter().any(|item| {
        matches!(item, OverlayItem::PreviewSegment { start, end }
            if start == p(60.0, 30.0) && end == p(10.0, -10.0))
    });
    assert!(closing, "the ring closes at the pointer");
    h.enter_all(&["60,30", "10,30", ""]);
    let [wipeout] = wipeouts(&h).try_into().unwrap();
    assert_eq!(wipeout.vertices.len(), 4);
    assert_eq!(orders(&h), [0, 0, 0, 1], "over the lines");
    h.enter_all(&["TEXT", "20,5", "5", "0", "Note"]);
    assert_eq!(
        *orders(&h).last().unwrap(),
        1,
        "the note in the wipeout's tier, over it"
    );
    h.undo();
    h.undo();
    assert!(wipeouts(&h).is_empty(), "one undo");
}

#[test]
fn a_wipeout_from_a_closed_polyline_and_frames_switched() {
    let mut h = Harness::new();
    h.enter_all(&["PLINE", "0,0", "40,0", "40,20", "C"]);
    h.enter_all(&["PLINE", "0,50", "40,50", ""]);
    h.enter_all(&["WIPEOUT", ""]);
    assert_eq!(h.prompt(), "Select a closed polyline:");
    h.click_world(p(20.0, 50.0));
    assert!(said(
        &h,
        "The polyline is open: a wipeout needs a closed one."
    ));
    h.click_world(p(20.0, 0.0));
    assert_eq!(h.prompt(), "Erase polyline? [Yes/No] <No>:");
    h.enter("Y");
    assert_eq!(wipeouts(&h).len(), 1);
    assert_eq!(h.entities().len(), 2, "the closed polyline is gone");

    assert!(h.session.drawing().settings().wipeout_frame);
    h.enter_all(&["WIPEOUT", "F", "OFF"]);
    assert!(said(&h, "Wipeout frames are hidden."));
    assert!(!h.session.drawing().settings().wipeout_frame);
    h.undo();
    assert!(h.session.drawing().settings().wipeout_frame, "one undo");
}

#[test]
fn a_wipeout_snaps_at_its_corners_and_the_middles_of_its_edges() {
    use crate::snap::{SnapMode, entity_snap_points};
    let wipeout = Entity::Wipeout(
        WipeoutEntity::new(vec![p(0.0, 0.0), p(40.0, 0.0), p(40.0, 20.0)]).unwrap(),
    );
    let tables = cad_document::Tables::none();
    let mut corners = entity_snap_points(&wipeout, SnapMode::Endpoint, tables);
    corners.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    corners.dedup();
    assert_eq!(corners, [p(0.0, 0.0), p(40.0, 0.0), p(40.0, 20.0)]);
    let middles = entity_snap_points(&wipeout, SnapMode::Midpoint, tables);
    assert_eq!(middles, [p(20.0, 0.0), p(40.0, 10.0), p(20.0, 10.0)]);
}
