//! ALIASEDIT and SETTINGS (#186): the user's own command aliases, edited
//! as text like `acad.pgp`, and the host's settings panel.
//!
//! The aliases live in the registry's table; the host keeps them between
//! sessions and gives them back through `Session::restore_settings`. The
//! settings — the toggles, the unit, the autosave delay, the theme — are
//! the host's to keep and show; the command only asks for its panel.

use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, HostRequest, Prompt, Step};

/// ALIASEDIT: opens the editor on the user's aliases, one per line as
/// `alias, command`, and takes what comes back. A line that cannot be
/// taken — a built-in name, an unknown command — is refused by its number
/// and the rest stand.
#[derive(Debug)]
pub(crate) struct AliasEditCommand {
    /// The aliases as the editor opened: what `start` found.
    text: String,
}

impl AliasEditCommand {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
        }
    }
}

impl Command for AliasEditCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.text = ctx.aliases.user_aliases();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Edit the aliases in the editor, one per line as alias, command")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::text()
    }

    fn editor(&self) -> Option<&str> {
        Some(&self.text)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Text(text) => {
                for refusal in ctx.aliases.set_user_aliases(&text) {
                    ctx.message(refusal);
                }
                let count = ctx.aliases.user_alias_count();
                let plural = if count == 1 { "" } else { "es" };
                ctx.message(format!("{count} alias{plural} defined."));
                Step::Finished
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}

/// SETTINGS: asks the host for its settings panel.
#[derive(Debug)]
pub(crate) struct SettingsCommand;

impl Command for SettingsCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::Settings);
        Step::Finished
    }

    immediate_command!();
}
