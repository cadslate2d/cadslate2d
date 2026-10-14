//! Groups through the session (#214): GROUP's options and refusals, a pick
//! selecting the whole group, PICKSTYLE and Ctrl+Shift+A, erasing a member
//! and undoing it, UNGROUP.

use cad_core::Point2;
use cad_document::Group;

use crate::testing::Harness;
use crate::{KeyAction, SelectionSummary};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn groups(h: &Harness) -> Vec<Group> {
    h.session
        .drawing()
        .groups()
        .iter()
        .map(|(_, group)| group.clone())
        .collect()
}

fn selected(h: &Harness) -> usize {
    h.session.selection().len()
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
fn a_selection_grouped_is_unnamed_and_a_pick_selects_the_whole_group() {
    let mut h = three_lines();
    let ids = h.ids();
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("G");
    assert!(said(&h, "Made the unnamed group *A1 of 2 objects."));
    assert_eq!(groups(&h)[0].members, vec![ids[0], ids[1]]);
    assert_eq!(selected(&h), 0);

    // One member picked: the group.
    h.click_world(p(50.0, 0.0));
    assert_eq!(selected(&h), 2);
    let summary = SelectionSummary::of(h.session.drawing(), h.session.selection());
    assert_eq!(summary.groups, vec!["*A1".to_owned()]);
    h.escape();
    // A window over part of it: the group too.
    h.select_box(p(-10.0, 15.0), p(110.0, 25.0));
    assert_eq!(selected(&h), 2);
    h.escape();

    // PICKSTYLE 0, or Ctrl+Shift+A: the one object.
    h.enter_all(&["PICKSTYLE"]);
    assert_eq!(h.prompt(), "Enter new value for PICKSTYLE (0 or 1) <1>:");
    h.enter("2");
    assert!(said(&h, "PICKSTYLE is 0 or 1."));
    h.enter("0");
    assert!(said(&h, "<Group selection off>"));
    h.click_world(p(50.0, 0.0));
    assert_eq!(selected(&h), 1);
    h.escape();
    h.session.key(KeyAction::GroupSelection);
    assert!(said(&h, "<Group selection on>"));
    h.click_world(p(50.0, 0.0));
    assert_eq!(selected(&h), 2);
}

#[test]
fn group_creates_lists_adds_removes_renames_and_explodes() {
    let mut h = three_lines();
    let ids = h.ids();
    h.enter("GROUP");
    assert_eq!(
        h.prompt(),
        "Enter a group option or [?/Add/Remove/Explode/REName/Selectable/Create] <Create>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Enter a group name <unnamed>:");
    h.enter("Front door");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("");
    assert!(said(&h, "Made the group \"Front door\" of 2 objects."));

    h.enter_all(&["G", "?"]);
    assert!(said(&h, "  \"Front door\": 2 objects"));
    h.escape();

    h.enter_all(&["G", "A", "front door"]);
    h.click_world(p(50.0, 40.0));
    h.enter("");
    assert!(said(&h, "1 object added to the group \"Front door\"."));
    assert_eq!(groups(&h)[0].members, ids);

    h.enter_all(&["G", "R", "Front door"]);
    assert_eq!(h.prompt(), "Select an object to remove from the group:");
    h.click_world(p(50.0, 20.0));
    h.enter("");
    assert!(said(
        &h,
        "Removed: the group \"Front door\" has 2 objects left."
    ));
    assert_eq!(groups(&h)[0].members, vec![ids[0], ids[2]]);

    h.enter_all(&["G", "REN", "Front door", "Door"]);
    assert!(said(&h, "The group \"Front door\" is renamed \"Door\"."));
    h.enter_all(&["G", "S", "Door"]);
    assert!(said(&h, "The group \"Door\" is not selectable."));
    h.click_world(p(50.0, 0.0));
    assert_eq!(
        selected(&h),
        1,
        "an unselectable group is picked by its objects"
    );
    h.escape();

    h.enter_all(&["G", "E", "door"]);
    assert!(said(
        &h,
        "The group \"Door\" is exploded: its 2 objects stay."
    ));
    assert!(groups(&h).is_empty());
    assert_eq!(h.ids().len(), 3);
}

#[test]
fn group_refuses_a_taken_name_a_bad_name_and_a_group_that_is_not_there() {
    let mut h = three_lines();
    h.enter_all(&["G", "", "Door"]);
    h.click_world(p(50.0, 0.0));
    h.enter("");
    h.enter_all(&["G", "", "DOOR"]);
    h.click_world(p(50.0, 20.0));
    h.enter("");
    assert!(said(&h, "There is already a group \"Door\"."));
    // What was selected for the refused group is still selected.
    h.escape();
    h.enter_all(&["G", "", "a*b"]);
    h.click_world(p(50.0, 20.0));
    h.enter("");
    h.escape();
    assert_eq!(groups(&h).len(), 1);
    assert!(said(&h, "A group name cannot contain \"*\"."));
    h.enter_all(&["G", "A", "Window"]);
    assert!(said(&h, "There is no group \"Window\"."));
    h.escape();
}

#[test]
fn erasing_a_member_shrinks_its_group_and_undo_brings_both_back() {
    let mut h = three_lines();
    let ids = h.ids();
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("G");

    // One member, with group selection off.
    h.enter_all(&["PICKSTYLE", "0"]);
    h.click_world(p(50.0, 0.0));
    h.enter("ERASE");
    assert_eq!(groups(&h)[0].members, vec![ids[1]]);
    h.undo();
    assert_eq!(groups(&h)[0].members, vec![ids[0], ids[1]]);

    // The whole group, picked as one: erased, it goes with its members.
    h.enter_all(&["PICKSTYLE", "1"]);
    h.click_world(p(50.0, 20.0));
    h.enter("ERASE");
    assert!(groups(&h).is_empty());
    assert_eq!(h.ids(), vec![ids[2]]);
    h.undo();
    assert_eq!(groups(&h)[0].members, vec![ids[0], ids[1]]);
}

#[test]
fn ungroup_explodes_the_groups_of_the_selection() {
    let mut h = three_lines();
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("G");
    h.click_world(p(50.0, 40.0));
    h.enter("UNGROUP");
    assert!(said(&h, "The selection holds no group."));
    h.click_world(p(50.0, 0.0));
    h.enter("UNGROUP");
    assert!(said(&h, "1 group exploded."));
    assert!(groups(&h).is_empty());
    h.undo();
    assert_eq!(groups(&h).len(), 1);
}
