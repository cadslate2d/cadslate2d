//! POLARANG: sets the angle between polar tracking directions (DESIGN §13.4).

use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// Shown as the prompt's default; filled in when the command starts.
#[derive(Debug, Default)]
pub(crate) struct PolarAngleCommand {
    current: f64,
}

/// `45`, `22.5`: as short as the value allows.
fn display(degrees: f64) -> String {
    let text = format!("{degrees:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Command for PolarAngleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.settings.polar_increment;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter polar angle increment").with_default(display(self.current))
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::angle()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Angle(degrees) if degrees > 0.0 && degrees <= 180.0 => {
                ctx.settings.polar_increment = degrees;
                ctx.message(format!("Polar angle increment: {}°", display(degrees)));
                Step::Finished
            }
            CommandInput::Angle(_) => {
                ctx.message("The increment must be greater than 0 and at most 180 degrees.");
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

    #[test]
    fn the_increment_is_set_by_typing_an_angle() {
        let mut h = Harness::new();
        h.enter("POLARANG");
        assert_eq!(h.prompt(), "Enter polar angle increment <90>:");
        h.enter("22.5");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.session.settings().polar_increment, 22.5);
        assert_eq!(h.log().last().unwrap(), "Polar angle increment: 22.5°");

        h.enter("POLARANG");
        assert_eq!(h.prompt(), "Enter polar angle increment <22.5>:");
    }

    #[test]
    fn enter_keeps_the_current_increment() {
        let mut h = Harness::new();
        h.enter_all(&["POLARANG", ""]);
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.session.settings().polar_increment, 90.0);
    }

    #[test]
    fn an_unusable_increment_is_refused_and_asked_again() {
        let mut h = Harness::new();
        h.enter("POLARANG");
        for bad in ["0", "-45", "270"] {
            h.enter(bad);
            assert_eq!(h.prompt(), "Enter polar angle increment <90>:", "{bad}");
        }
        assert_eq!(h.session.settings().polar_increment, 90.0);
        h.enter("30");
        assert_eq!(h.session.settings().polar_increment, 30.0);
    }
}
