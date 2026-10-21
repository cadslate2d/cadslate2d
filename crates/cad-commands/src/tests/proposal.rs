//! Proposals (#254): an intent run against a copy, shown by the overlay,
//! applied on Accept as one undo, dropped on Reject without a trace.

use cad_core::Point2;
use cad_document::{CircleEntity, Entity, LineEntity};
use cad_intent::{Intent, VERSION, parse};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn document(operations: &str) -> Intent {
    parse(&format!(
        r#"{{"version": {VERSION}, "operations": [{operations}]}}"#
    ))
    .unwrap()
}

fn with_a_line() -> Harness {
    Harness::with_entities([Entity::Line(LineEntity::new(
        Point2::new(0.0, 0.0),
        Point2::new(100.0, 0.0),
    ))])
}

fn proposal_items(h: &Harness) -> (usize, usize) {
    let items = h.session.overlay();
    let added = items
        .iter()
        .filter(|item| matches!(item, OverlayItem::ProposalAdded(_)))
        .count();
    let removed = items
        .iter()
        .filter(|item| matches!(item, OverlayItem::ProposalRemoved(_)))
        .count();
    (added, removed)
}

#[test]
fn a_proposal_leaves_the_drawing_and_its_revision_untouched() {
    let mut h = with_a_line();
    let revision = h.session.drawing().revision();
    let dirty = h.session.propose(document(
        r#"{"op": "circle", "center": [50, 50], "radius": 10},
           {"op": "move", "targets": [1], "displacement": [0, 20]}"#,
    ));
    assert!(dirty.contains(Dirty::OVERLAY));
    assert!(!dirty.contains(Dirty::SCENE));
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.session.drawing().len(), 1);
    assert!(!h.session.drawing().can_undo());

    let proposal = h.session.proposal().expect("a proposal");
    assert_eq!(proposal.added.len(), 1);
    assert_eq!(proposal.changed.len(), 1);
    assert!(proposal.removed.is_empty());
    assert!(matches!(
        &proposal.added[0].1,
        Entity::Circle(c) if *c == CircleEntity::new(Point2::new(50.0, 50.0), 10.0)
    ));
    let summary = proposal.summary();
    assert_eq!(summary.headline, "Adds 1 object, changes 1 object");
    assert_eq!(summary.error, None);
    assert_eq!(summary.operations.len(), 2);
    // The circle and the moved line in the proposal's style; the line as
    // it is now in the removal's.
    assert_eq!(proposal_items(&h), (2, 1));
    // The pointer moves; the proposal stays, and the drawing too.
    h.move_to(10.0, 10.0);
    assert_eq!(proposal_items(&h), (2, 1));
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn accept_is_one_undo_and_makes_what_was_shown() {
    let mut h = with_a_line();
    h.session.propose(document(
        r#"{"op": "circle", "center": [50, 50], "radius": 10},
           {"op": "copy", "targets": [1], "displacement": [0, 30]},
           {"op": "move", "targets": [1], "displacement": [0, 20]}"#,
    ));
    let shown: Vec<Entity> = {
        let proposal = h.session.proposal().unwrap();
        proposal
            .added
            .iter()
            .map(|(_, e)| e.clone())
            .chain(proposal.changed.iter().map(|(_, _, after)| after.clone()))
            .collect()
    };
    let dirty = h.session.accept_proposal();
    assert!(dirty.contains(Dirty::SCENE));
    assert!(h.session.proposal().is_none());
    assert_eq!(proposal_items(&h), (0, 0));
    assert_eq!(h.session.drawing().len(), 3);
    for entity in &shown {
        assert!(h.entities().contains(entity), "{entity:?}");
    }
    assert_eq!(h.log().last().unwrap(), "Proposal accepted.");

    // One undo takes all three operations back.
    h.undo();
    assert_eq!(h.session.drawing().len(), 1);
    assert_eq!(
        h.lines()[0],
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(100.0, 0.0))
    );
    assert!(!h.session.drawing().can_undo());
    h.redo();
    assert_eq!(h.session.drawing().len(), 3);
}

#[test]
fn reject_leaves_no_trace_and_a_new_proposal_replaces_the_old() {
    let mut h = with_a_line();
    let revision = h.session.drawing().revision();
    h.session.propose(document(
        r#"{"op": "circle", "center": [0, 0], "radius": 5}"#,
    ));
    h.session
        .propose(document(r#"{"op": "line", "points": [[0, 10], [10, 10]]}"#));
    assert_eq!(h.session.proposal().unwrap().added.len(), 1);
    assert!(matches!(
        h.session.proposal().unwrap().added[0].1,
        Entity::Line(_)
    ));
    let dirty = h.session.reject_proposal();
    assert!(dirty.contains(Dirty::OVERLAY));
    assert!(h.session.proposal().is_none());
    assert_eq!(proposal_items(&h), (0, 0));
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(!h.session.drawing().can_undo());
    assert_eq!(h.session.reject_proposal(), Dirty::NONE);
    assert_eq!(h.session.accept_proposal(), Dirty::NONE);
}

#[test]
fn a_proposal_that_stops_says_why_and_shows_what_ran_before() {
    let mut h = with_a_line();
    h.session.propose(document(
        r#"{"op": "circle", "center": [0, 0], "radius": 5},
           {"op": "move", "targets": [99], "displacement": [1, 1]}"#,
    ));
    let summary = h.session.proposal().unwrap().summary();
    assert_eq!(summary.added, 1);
    assert_eq!(
        summary.error.as_deref(),
        Some("operation 1: no object 99 in the drawing")
    );
    // Removing is shown too.
    let mut h = with_a_line();
    h.session.propose(document(
        r#"{"op": "line", "points": [[50, -20], [50, 20]]},
           {"op": "trim", "edges": [1], "targets": [{"id": 2, "near": [50, -15]}]}"#,
    ));
    let proposal = h.session.proposal().unwrap();
    assert_eq!(proposal.added.len(), 1, "the trimmed line, as it ends up");
    assert_eq!(proposal.summary().headline, "Adds 1 object");
}

#[test]
fn opening_another_drawing_drops_the_proposal() {
    let mut h = with_a_line();
    h.session.propose(document(
        r#"{"op": "circle", "center": [0, 0], "radius": 5}"#,
    ));
    h.session.new_drawing();
    assert!(h.session.proposal().is_none());
}
