//! UNDO and REDO: immediate commands that step through the linear history.

use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Step};

/// Immediate commands finish in `start`, so these are never consulted.
macro_rules! immediate_command_boilerplate {
    () => {
        fn prompt(&self) -> String {
            String::new()
        }

        fn expected(&self) -> ExpectedInput {
            ExpectedInput::Command
        }

        fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
            Step::Finished
        }
    };
}

#[derive(Debug)]
pub(crate) struct UndoCommand;

impl Command for UndoCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match ctx.drawing.undo() {
            // Selection reconciliation (DESIGN §19.7): entities that undo brought
            // back — an undone erase — become selected. Entities that undo removed
            // are dropped from the selection by the session after every step.
            Some(report) => report
                .inserted
                .into_iter()
                .for_each(|id| ctx.selection.add(id)),
            None => ctx.message("Nothing to undo."),
        }
        Step::Finished
    }

    immediate_command_boilerplate!();
}

#[derive(Debug)]
pub(crate) struct RedoCommand;

impl Command for RedoCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.drawing.redo().is_none() {
            ctx.message("Nothing to redo.");
        }
        Step::Finished
    }

    immediate_command_boilerplate!();
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::LineEntity;

    use crate::Dirty;
    use crate::testing::Harness;

    fn drawn() -> Harness {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "100,0", ""]);
        h
    }

    /// DESIGN §41, verbatim:
    /// Given empty drawing / When LINE receives 0,0 and 100,0 / Then one line
    /// exists / And undo removes it / And redo restores the same EntityId /
    /// And next_entity_id never decreases.
    #[test]
    fn the_design_document_command_example() {
        let mut h = Harness::new();
        let mut high_water = h.session.drawing().next_entity_id();
        let mut check_allocator = |h: &Harness| {
            let next = h.session.drawing().next_entity_id();
            assert!(
                next >= high_water,
                "next_entity_id decreased from {high_water} to {next}"
            );
            high_water = next;
        };

        h.enter_all(&["LINE", "0,0", "100,0", ""]);
        check_allocator(&h);
        assert_eq!(
            h.lines(),
            vec![LineEntity::new(
                Point2::new(0.0, 0.0),
                Point2::new(100.0, 0.0)
            )]
        );
        let id = h.ids()[0];

        h.enter("UNDO");
        check_allocator(&h);
        assert!(h.lines().is_empty());

        h.enter("REDO");
        check_allocator(&h);
        assert_eq!(h.ids(), vec![id]);
    }

    #[test]
    fn undo_and_redo_are_available_as_commands_aliases_and_shortcuts() {
        for undo in ["UNDO", "undo", "U"] {
            let mut h = drawn();
            assert!(h.enter(undo).contains(Dirty::SCENE));
            assert!(h.lines().is_empty(), "typed {undo:?}");
        }
        let mut h = drawn();
        assert!(h.undo().contains(Dirty::SCENE));
        assert!(h.lines().is_empty());
        assert!(h.redo().contains(Dirty::SCENE));
        assert_eq!(h.lines().len(), 1);
        h.undo();
        h.enter("REDO");
        assert_eq!(h.lines().len(), 1);
    }

    #[test]
    fn nothing_to_undo_or_redo_is_reported() {
        let mut h = Harness::new();
        assert!(!h.enter("U").contains(Dirty::SCENE));
        assert_eq!(h.log().last().unwrap(), "Nothing to undo.");
        assert!(!h.enter("REDO").contains(Dirty::SCENE));
        assert_eq!(h.log().last().unwrap(), "Nothing to redo.");
        assert_eq!(h.prompt(), "Command:");
    }

    /// DESIGN §26.3 criterion 15.
    #[test]
    fn new_geometry_after_undo_discards_the_redo_branch_without_reusing_ids() {
        let mut h = drawn();
        let first = h.ids()[0];
        h.undo();
        h.enter_all(&["L", "0,10", "100,10", ""]);

        let second = h.ids()[0];
        assert!(second > first, "{second} must not reuse {first}");
        h.redo();
        assert_eq!(h.log().last().unwrap(), "Nothing to redo.");
        assert_eq!(h.ids(), vec![second]);
    }

    #[test]
    fn ui_state_reports_undo_and_redo_availability() {
        let mut h = drawn();
        let state = h.session.ui_state();
        assert!(state.can_undo && !state.can_redo);
        h.undo();
        let state = h.session.ui_state();
        assert!(!state.can_undo && state.can_redo);
    }

    #[test]
    fn the_undo_shortcut_cancels_a_command_in_progress_first() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "50,0"]);
        h.undo();
        assert_eq!(h.prompt(), "Command:");
        assert!(h.lines().is_empty());
        assert!(h.log().contains(&"*Cancel*".to_owned()));
    }
}
