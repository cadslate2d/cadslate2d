//! Selection reconciliation through undo and redo (DESIGN §19.7).
//!
//! M0 ships no erase or modify command, so these tests register two tiny
//! commands of their own. That also exercises the registry as an extension
//! point: nothing in the session had to change to add them.

use cad_core::{EntityId, Point2, Vec2};
use cad_document::{Drawing, Entity, LineEntity};
use cad_input::ExpectedInput;

use crate::testing::Harness;
use crate::{Command, CommandContext, CommandInput, CommandRegistry, Session, Step};

/// Erases the selected entities in one transaction.
#[derive(Debug)]
struct EraseSelection;

/// Moves the selected entities by (0, 10) in one transaction.
#[derive(Debug)]
struct NudgeSelection;

fn edit_selection(ctx: &mut CommandContext<'_>, edit: impl Fn(&Entity) -> Option<Entity>) {
    let ids: Vec<EntityId> = ctx.selection.iter().collect();
    let edits: Vec<(EntityId, Option<Entity>)> = ids
        .iter()
        .filter_map(|&id| Some((id, edit(ctx.drawing.entity(id)?))))
        .collect();
    let mut transaction = ctx.drawing.transaction();
    for (id, replacement) in edits {
        match replacement {
            Some(entity) => transaction.replace(id, entity).unwrap(),
            None => transaction.remove(id).unwrap(),
        }
    }
    transaction.commit();
}

macro_rules! immediate {
    ($command:ty, $edit:expr) => {
        impl Command for $command {
            fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
                edit_selection(ctx, $edit);
                Step::Finished
            }
            fn prompt(&self) -> String {
                String::new()
            }
            fn expected(&self) -> ExpectedInput {
                ExpectedInput::Command
            }
            fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
                Step::Finished
            }
        }
    };
}

immediate!(EraseSelection, |_| None);
immediate!(NudgeSelection, |entity| {
    let Entity::Line(line) = entity;
    let offset = Vec2::new(0.0, 10.0);
    Some(Entity::Line(LineEntity::new(
        line.start + offset,
        line.end + offset,
    )))
});

/// One line, selected, in a session that also knows the two test commands.
fn selected_line() -> (Harness, EntityId) {
    let mut registry = CommandRegistry::standard();
    registry
        .register("TEST-ERASE", &[], || Box::new(EraseSelection))
        .unwrap();
    registry
        .register("TEST-NUDGE", &[], || Box::new(NudgeSelection))
        .unwrap();
    let mut session = Session::with_registry(Drawing::new(), registry);
    session.resize(800.0, 600.0);

    let mut h = Harness { session };
    h.enter_all(&["L", "-100,0", "100,0", ""]);
    h.click_world(Point2::new(0.0, 0.0));
    let id = h.ids()[0];
    assert_eq!(selected(&h), vec![id]);
    (h, id)
}

fn selected(h: &Harness) -> Vec<EntityId> {
    h.session.selection().iter().collect()
}

/// The invariant behind every case below.
fn assert_selection_is_alive(h: &Harness) {
    for id in h.session.selection().iter() {
        assert!(
            h.session.drawing().entity(id).is_some(),
            "selection names dead entity {id}"
        );
    }
}

#[test]
fn undo_create_removes_the_entity_from_the_selection() {
    let (mut h, _) = selected_line();
    h.undo();
    assert!(h.lines().is_empty());
    assert!(selected(&h).is_empty());
}

#[test]
fn redo_create_does_not_select_the_entity_again() {
    let (mut h, id) = selected_line();
    h.undo();
    h.redo();
    assert_eq!(h.ids(), vec![id]);
    assert!(selected(&h).is_empty());
}

#[test]
fn undo_erase_restores_the_same_entity_and_selects_it() {
    let (mut h, id) = selected_line();
    h.enter("TEST-ERASE");
    assert!(h.lines().is_empty());
    assert!(
        selected(&h).is_empty(),
        "an erased entity cannot stay selected"
    );

    h.undo();
    assert_eq!(h.ids(), vec![id], "restored with the same EntityId");
    assert_eq!(
        selected(&h),
        vec![id],
        "the restored entity becomes selected"
    );
}

#[test]
fn undo_erase_selects_the_restored_entity_even_if_nothing_was_selected() {
    let (mut h, id) = selected_line();
    h.enter("TEST-ERASE");
    h.enter_all(&["L", "0,50", "10,50", ""]);
    h.undo(); // undo the new line
    h.undo(); // undo the erase
    assert_eq!(selected(&h), vec![id]);
}

#[test]
fn redo_erase_drops_the_entity_from_the_selection() {
    let (mut h, _) = selected_line();
    h.enter("TEST-ERASE");
    h.undo();
    h.redo();
    assert!(h.lines().is_empty());
    assert!(selected(&h).is_empty());
}

#[test]
fn undo_and_redo_modify_keep_the_entity_selected() {
    let (mut h, id) = selected_line();
    h.enter("TEST-NUDGE");
    assert_eq!(h.lines()[0].start, Point2::new(-100.0, 10.0));
    assert_eq!(selected(&h), vec![id]);

    h.undo();
    assert_eq!(
        h.lines()[0].start,
        Point2::new(-100.0, 0.0),
        "previous geometry is restored"
    );
    assert_eq!(selected(&h), vec![id]);

    h.redo();
    assert_eq!(h.lines()[0].start, Point2::new(-100.0, 10.0));
    assert_eq!(selected(&h), vec![id]);
}

#[test]
fn the_selection_never_names_a_dead_entity_through_any_history_walk() {
    let (mut h, _) = selected_line();
    let walk = [
        "TEST-NUDGE",
        "TEST-ERASE",
        "UNDO",
        "UNDO",
        "UNDO",
        "REDO",
        "click",
        "REDO",
        "REDO",
        "UNDO",
    ];
    for step in walk {
        match step {
            "click" => h.click_world(Point2::new(0.0, 0.0)),
            command => h.enter(command),
        };
        assert_selection_is_alive(&h);
    }
}

#[test]
fn selection_is_not_part_of_history() {
    let (mut h, id) = selected_line();
    // Changing the selection creates no undo step: one undo still removes the line.
    h.escape();
    h.click_world(Point2::new(0.0, 0.0));
    h.escape();
    h.undo();
    assert!(h.lines().is_empty());
    h.redo();
    assert_eq!(h.ids(), vec![id]);
}
