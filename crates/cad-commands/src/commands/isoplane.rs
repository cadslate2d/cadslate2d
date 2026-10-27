//! ISOPLANE (#336): the isometric plane ORTHO and isocircles work on —
//! Left, Top, or Right; Enter takes the next, as F5 and Ctrl+E do.

use cad_core::iso::Plane;
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LEFT: Keyword = Keyword::new("Left");
const TOP: Keyword = Keyword::new("Top");
const RIGHT: Keyword = Keyword::new("Right");

#[derive(Debug, Default)]
pub(crate) struct IsoplaneCommand {
    /// The plane Enter takes: the one after the current.
    next: Plane,
}

/// Makes `plane` current and says so.
pub(crate) fn set_isoplane(plane: Plane, ctx: &mut CommandContext<'_>) {
    ctx.settings.isoplane = plane;
    ctx.message(format!("Current isoplane: {}", plane.name()));
}

impl Command for IsoplaneCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.next = ctx.settings.isoplane.next();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter isometric plane setting").with_default(self.next.name())
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing).with_keywords(&[LEFT, TOP, RIGHT])
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let plane = match input {
            CommandInput::Keyword(k) if k == LEFT => Plane::Left,
            CommandInput::Keyword(k) if k == TOP => Plane::Top,
            CommandInput::Keyword(k) if k == RIGHT => Plane::Right,
            CommandInput::Accept => self.next,
            _ => return Step::Continue,
        };
        set_isoplane(plane, ctx);
        Step::Finished
    }
}
