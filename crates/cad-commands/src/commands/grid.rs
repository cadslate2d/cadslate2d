//! GRID: sets the spacing of the reference grid (DESIGN §13.4). F7 shows and
//! hides it.

use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// Shown as the prompt's default; filled in when the command starts.
#[derive(Debug, Default)]
pub(crate) struct GridCommand {
    current: f64,
}

/// `10`, `2.5`: as short as the value allows.
fn display(spacing: f64) -> String {
    let text = format!("{spacing:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Command for GridCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.settings.grid_spacing;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Specify grid spacing").with_default(display(self.current))
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::distance()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Distance(spacing) if spacing.is_finite() && spacing > 0.0 => {
                ctx.settings.grid_spacing = spacing;
                ctx.message(format!("Grid spacing: {}", display(spacing)));
                Step::Finished
            }
            CommandInput::Distance(_) => {
                ctx.message("The spacing must be greater than zero.");
                Step::Continue
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::Harness;
    use crate::{OverlayItem, Toggle};

    #[test]
    fn the_spacing_is_set_by_typing_a_distance() {
        let mut h = Harness::new();
        h.enter("GRID");
        assert_eq!(h.prompt(), "Specify grid spacing <10>:");
        h.enter("2.5");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.session.settings().grid_spacing, 2.5);
        assert_eq!(h.log().last().unwrap(), "Grid spacing: 2.5");
    }

    #[test]
    fn enter_keeps_the_spacing_and_nonsense_is_refused() {
        let mut h = Harness::new();
        h.enter_all(&["GRID", "0"]);
        assert_eq!(h.prompt(), "Specify grid spacing <10>:");
        h.enter("-5");
        assert_eq!(h.prompt(), "Specify grid spacing <10>:");
        h.enter("");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.session.settings().grid_spacing, 10.0);
    }

    #[test]
    fn the_overlay_describes_the_grid_only_while_it_is_on() {
        let mut h = Harness::new();
        assert!(
            h.session.overlay().is_empty(),
            "the harness starts without a grid"
        );

        h.toggle(Toggle::Grid);
        assert_eq!(
            h.session.overlay(),
            vec![OverlayItem::Grid { spacing: 10.0 }]
        );
        h.enter_all(&["GRID", "25"]);
        assert_eq!(h.session.overlay()[0], OverlayItem::Grid { spacing: 25.0 });

        h.toggle(Toggle::Grid);
        assert!(h.session.overlay().is_empty());
    }

    #[test]
    fn the_grid_is_drafting_state_not_part_of_the_drawing() {
        let mut h = Harness::new();
        let revision = h.session.drawing().revision();
        h.toggle(Toggle::Grid);
        h.enter_all(&["GRID", "5"]);
        assert_eq!(h.session.drawing().revision(), revision);
        assert!(!h.session.ui_state().can_undo);
        assert!(!h.session.ui_state().unsaved_changes);
    }
}
