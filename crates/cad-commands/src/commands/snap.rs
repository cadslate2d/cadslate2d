//! SNAP (#219): the snap grid's spacing, and grid snap on or off. F9 and
//! the status bar switch it too.
//!
//! A spacing typed sets both directions and switches snap on; `Aspect`
//! asks for them apart. `Style` chooses the standard grid or the
//! isometric one (#336), whose lattice runs along 30°, 150°, and 90° at
//! the vertical spacing. What grid snap does to the pointer is in
//! `aids::grid_snap`, applied in `Session::aim`.

use cad_input::{ExpectedInput, Keyword};

use super::grid::display;
use crate::{Command, CommandContext, CommandInput, Prompt, Step, Toggle};

const ON: Keyword = Keyword::new("ON");
const OFF: Keyword = Keyword::new("OFF");
const ASPECT: Keyword = Keyword::new("Aspect");
const STYLE: Keyword = Keyword::new("Style");
const STANDARD: Keyword = Keyword::new("Standard");
const ISOMETRIC: Keyword = Keyword::new("Isometric");

#[derive(Debug)]
enum State {
    Spacing,
    Across,
    Up(f64),
    Style,
    /// The isometric lattice's vertical spacing.
    IsoSpacing,
}

#[derive(Debug)]
pub(crate) struct SnapCommand {
    state: State,
    /// The spacing as the command found it: the prompts' defaults.
    current: [f64; 2],
}

impl SnapCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Spacing,
            current: [0.0; 2],
        }
    }

    /// Sets the spacing, switches snap on, and says so.
    fn set(spacing: [f64; 2], ctx: &mut CommandContext<'_>) -> Step {
        ctx.settings.snap_spacing = spacing;
        if !ctx.toggles.is_on(Toggle::Snap) {
            ctx.toggles.flip(Toggle::Snap);
        }
        let [across, up] = spacing;
        ctx.message(if display(across) == display(up) {
            format!("Snap spacing: {}", display(across))
        } else {
            format!(
                "Snap spacing: {} across, {} up",
                display(across),
                display(up)
            )
        });
        Step::Finished
    }

    fn switch(on: bool, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.toggles.is_on(Toggle::Snap) != on {
            ctx.toggles.flip(Toggle::Snap);
        }
        ctx.message(if on { "<Snap on>" } else { "<Snap off>" });
        Step::Finished
    }
}

/// A spacing a grid can have.
fn usable(spacing: f64) -> bool {
    spacing.is_finite() && spacing > 0.0
}

impl Command for SnapCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.settings.snap_spacing;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Spacing => {
                let [across, up] = self.current;
                let default = if display(across) == display(up) {
                    display(across)
                } else {
                    format!("{}, {}", display(across), display(up))
                };
                Prompt::new("Specify snap spacing").with_default(default)
            }
            State::Across => {
                Prompt::new("Specify horizontal spacing").with_default(display(self.current[0]))
            }
            State::Up(_) | State::IsoSpacing => {
                Prompt::new("Specify vertical spacing").with_default(display(self.current[1]))
            }
            State::Style => Prompt::new("Enter snap grid style").with_default("Standard"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Spacing => ExpectedInput::distance().with_keywords(&[ON, OFF, ASPECT, STYLE]),
            State::Across | State::Up(_) | State::IsoSpacing => ExpectedInput::distance(),
            State::Style => ExpectedInput::new(cad_input::InputKind::Nothing)
                .with_keywords(&[STANDARD, ISOMETRIC]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Spacing, CommandInput::Keyword(ON)) => return Self::switch(true, ctx),
            (State::Spacing, CommandInput::Keyword(OFF)) => return Self::switch(false, ctx),
            (State::Spacing, CommandInput::Keyword(STYLE)) => self.state = State::Style,
            (State::Spacing, CommandInput::Keyword(_)) => self.state = State::Across,
            (State::Style, CommandInput::Keyword(ISOMETRIC)) => self.state = State::IsoSpacing,
            (State::Style, CommandInput::Keyword(_) | CommandInput::Accept) => {
                ctx.settings.isometric = false;
                ctx.message("Snap style: Standard");
                return Step::Finished;
            }
            (State::IsoSpacing, input @ (CommandInput::Distance(_) | CommandInput::Accept)) => {
                let spacing = match input {
                    CommandInput::Distance(spacing) => spacing,
                    _ => self.current[1],
                };
                if !usable(spacing) {
                    ctx.message("The spacing must be greater than zero.");
                    return Step::Continue;
                }
                ctx.settings.isometric = true;
                ctx.settings.snap_spacing = [spacing, spacing];
                if !ctx.toggles.is_on(Toggle::Snap) {
                    ctx.toggles.flip(Toggle::Snap);
                }
                ctx.message(format!(
                    "Snap style: Isometric, {} up. Isoplane: {}",
                    display(spacing),
                    ctx.settings.isoplane.name()
                ));
                return Step::Finished;
            }
            (State::Spacing, CommandInput::Distance(spacing)) if usable(spacing) => {
                return Self::set([spacing, spacing], ctx);
            }
            (State::Spacing, CommandInput::Accept) => return Self::set(self.current, ctx),
            (State::Across, CommandInput::Distance(across)) if usable(across) => {
                self.state = State::Up(across);
            }
            (State::Across, CommandInput::Accept) => self.state = State::Up(self.current[0]),
            (State::Up(across), CommandInput::Distance(up)) if usable(up) => {
                return Self::set([*across, up], ctx);
            }
            (State::Up(across), CommandInput::Accept) => {
                return Self::set([*across, self.current[1]], ctx);
            }
            (_, CommandInput::Distance(_)) => {
                ctx.message("The spacing must be greater than zero.");
            }
            _ => {}
        }
        Step::Continue
    }
}
