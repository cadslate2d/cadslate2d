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

/// A proposal is brought into view when it is not (#358): an A3 sheet far
/// larger than the screen is fitted, and ZOOM Previous goes back; a small
/// one already in view moves nothing.
#[test]
fn a_proposal_out_of_view_is_brought_into_it_and_one_in_view_is_not() {
    let mut h = with_a_line();
    // Most of the view is about the line: the circle near it is in view.
    let before = h.session.view().clone();
    let dirty = h.session.propose(document(
        r#"{"op": "circle", "center": [50, 0], "radius": 5}"#,
    ));
    assert!(!dirty.contains(Dirty::VIEW));
    assert_eq!(*h.session.view(), before);
    h.session.reject_proposal();
    // A large rectangle far off: fitted, whole.
    let dirty = h.session.propose(document(
        r#"{"op": "rectangle", "corner": [100000, 100000], "opposite": [520000, 397000]}"#,
    ));
    assert!(dirty.contains(Dirty::VIEW));
    let visible = h.session.view().visible_bounds();
    assert!(visible.min().x <= 100000.0 && visible.max().x >= 520000.0);
    assert!(visible.min().y <= 100000.0 && visible.max().y >= 397000.0);
    // The drawing is as it was; ZOOM Previous returns to the view left.
    assert_eq!(h.session.drawing().len(), 1);
    h.session.reject_proposal();
    h.enter_all(&["ZOOM", "P"]);
    assert_eq!(*h.session.view(), before);
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

/// A hatch in a sheet's view with no closed outline around its point is
/// skipped, not the end of the proposal (#358): the summary says which,
/// the rest is drawn, and Accept applies it as one undo. Only in a view:
/// the same hatch on its own still stops the proposal.
#[test]
fn a_hatch_in_a_view_with_no_outline_is_skipped_and_said() {
    let sheet = r#"{"op": "title_block", "paper": "A3", "scale": 1, "title": "X", "at": [0, 0]},
           {"op": "view", "id": "front", "placed": "principal", "operations": [
               {"op": "rectangle", "corner": [0, 0], "opposite": [60, 40]},
               {"op": "hatch", "pattern": "ANSI31", "inside": [30, 20]},
               {"op": "line", "points": [[100, 0], [150, 0], [150, 30]]},
               {"op": "hatch", "pattern": "ANSI31", "inside": [125, 10]}]}"#;
    let hatches = |entities: &[Entity]| {
        entities
            .iter()
            .filter(|e| matches!(e, Entity::Hatch(_)))
            .count()
    };
    let mut h = Harness::new();
    h.session.propose(document(sheet));
    let summary = h.session.proposal().unwrap().summary();
    assert_eq!(summary.error, None);
    assert!(
        summary.headline.ends_with("; 1 hatch skipped"),
        "{}",
        summary.headline
    );
    assert!(
        summary
            .operations
            .iter()
            .any(|o| o
                == "HATCH: skipped, a hatch in view front: no closed outline surrounds its point"),
        "{:?}",
        summary.operations
    );
    h.session.accept_proposal();
    assert_eq!(hatches(&h.entities()), 1);
    h.undo();
    assert_eq!(h.session.drawing().len(), 0);

    let mut h = Harness::new();
    h.session.propose(document(
        r#"{"op": "line", "points": [[100, 0], [150, 0], [150, 30]]},
           {"op": "hatch", "pattern": "ANSI31", "inside": [125, 10]}"#,
    ));
    let summary = h.session.proposal().unwrap().summary();
    assert!(
        summary
            .error
            .as_deref()
            .is_some_and(|e| e.contains("HATCH")),
        "{summary:?}"
    );
    assert!(!summary.headline.contains("skipped"));
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

/// #260: what a request may carry is the selection as the Properties panel
/// describes it, with the IDs an intent refers to, and the layers.
#[test]
fn the_ai_context_describes_the_selection_with_its_ids_and_the_layers() {
    let mut h = with_a_line();
    h.enter_all(&["LAYER", "N", "HOLES", ""]);
    h.enter_all(&["C", "30,30", "8"]);
    let context = h.session.ai_context();
    assert!(context.objects.is_empty(), "nothing is selected");
    assert_eq!(context.layers, ["0", "HOLES"]);
    h.enter("ALL");
    let context = h.session.ai_context();
    assert_eq!(context.objects.len(), 2);
    assert_eq!(context.objects[0].id, 1);
    assert_eq!(context.objects[0].kind, "line");
    assert!(
        context.objects[0]
            .geometry
            .starts_with("start 0.0000, 0.0000; end 100.0000, 0.0000"),
        "{}",
        context.objects[0].geometry
    );
    assert_eq!(context.objects[0].layer.as_deref(), Some("0"));
    assert_eq!(context.objects[1].kind, "circle");
    assert!(
        context.objects[1].geometry.contains("radius 8.0000"),
        "{}",
        context.objects[1].geometry
    );
    assert_eq!(context.left_out, 0);

    // A polyline's vertices, where an edit needs them, and its bulges.
    h.escape();
    h.escape();
    h.enter_all(&["PLINE", "0,50", "40,50", "A", "40,90", ""]);
    h.enter("SELECT");
    h.enter_all(&["L", ""]);
    let context = h.session.ai_context();
    assert_eq!(context.objects[0].kind, "polyline");
    assert!(
        context.objects[0].geometry.ends_with(
            "points (0.0000, 50.0000), (40.0000, 50.0000) bulge 1.0000, (40.0000, 90.0000)"
        ),
        "{}",
        context.objects[0].geometry
    );
}
