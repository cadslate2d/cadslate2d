//! OSNAP: chooses which object snaps run while OSNAP is on (DESIGN §18).
//!
//! A narrow, task-specific setting (§5): each mode is a keyword that flips
//! it, and Enter finishes. F3 switches all running snaps on and off without
//! changing this choice.

use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, SnapMode, Step};

const MODES: [(Keyword, SnapMode); 6] = [
    (Keyword::new("Endpoint"), SnapMode::Endpoint),
    (Keyword::new("Midpoint"), SnapMode::Midpoint),
    (Keyword::new("Center"), SnapMode::Center),
    (Keyword::new("Quadrant"), SnapMode::Quadrant),
    (Keyword::new("Intersection"), SnapMode::Intersection),
    (Keyword::new("Nearest"), SnapMode::Nearest),
];

#[derive(Debug)]
pub(crate) struct OsnapCommand;

fn report(ctx: &mut CommandContext<'_>) {
    let on: Vec<&str> = ctx
        .settings
        .snap_modes
        .enabled()
        .map(SnapMode::label)
        .collect();
    let list = if on.is_empty() {
        "none".to_owned()
    } else {
        on.join(", ")
    };
    ctx.message(format!("Running object snaps: {list}"));
}

impl Command for OsnapCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        report(ctx);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Press Enter to finish")
    }

    fn expected(&self) -> ExpectedInput {
        let keywords: Vec<Keyword> = MODES.iter().map(|(keyword, _)| *keyword).collect();
        ExpectedInput::new(InputKind::Nothing).with_keywords(&keywords)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Keyword(keyword) => {
                if let Some((_, mode)) = MODES.iter().find(|(candidate, _)| *candidate == keyword) {
                    ctx.settings.snap_modes.flip(*mode);
                    report(ctx);
                }
                Step::Continue
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::SnapMode;
    use crate::testing::Harness;

    #[test]
    fn modes_are_flipped_by_keyword_and_enter_finishes() {
        let mut h = Harness::new();
        h.enter("OSNAP");
        assert_eq!(
            h.prompt(),
            "Press Enter to finish or [Endpoint/Midpoint/Center/Quadrant/Intersection/Nearest]:"
        );
        assert!(h.log().iter().any(|line| line
            == "Running object snaps: Endpoint, Intersection, Midpoint, Center, Quadrant"));

        h.enter_all(&["N", "endpoint", ""]);
        assert_eq!(h.prompt(), "Command:");
        let modes = h.session.snap_modes();
        assert!(modes.is_on(SnapMode::Nearest));
        assert!(!modes.is_on(SnapMode::Endpoint));
        assert!(h.log().iter().any(|line| line
            == "Running object snaps: Intersection, Midpoint, Center, Quadrant, Nearest"));
    }

    #[test]
    fn every_mode_can_be_switched_off() {
        let mut h = Harness::new();
        h.enter_all(&["OS", "E", "M", "C", "Q", "I", ""]);
        assert_eq!(h.session.snap_modes(), crate::SnapModes::NONE);
        assert!(
            h.log()
                .iter()
                .any(|line| line == "Running object snaps: none")
        );
    }

    #[test]
    fn the_choice_is_session_state_and_not_undoable() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "", "OSNAP", "N", ""]);
        h.undo();
        assert!(
            h.lines().is_empty(),
            "undo reverts the line, not the setting"
        );
        assert!(h.session.snap_modes().is_on(SnapMode::Nearest));
    }
}
