//! The command registry.
//!
//! Commands are registered, not hard-coded: adding one touches this table and
//! nothing in the resolver or the session (open/closed principle). Typed
//! input, shortcuts, and UI actions all reach commands through it.

use std::collections::HashMap;

use cad_input::{CommandAliases, DuplicateCommandName};

use crate::Command;
use crate::commands::{
    ArcCommand, CircleCommand, ExportCommand, LineCommand, OpenCommand, RedoCommand, UndoCommand,
};

type CommandFactory = fn() -> Box<dyn Command>;

struct Registration {
    name: &'static str,
    factory: CommandFactory,
}

pub struct CommandRegistry {
    aliases: CommandAliases,
    registrations: HashMap<String, Registration>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.registrations.values().map(|r| r.name).collect();
        names.sort_unstable();
        f.debug_struct("CommandRegistry")
            .field("commands", &names)
            .finish()
    }
}

impl CommandRegistry {
    pub fn empty() -> Self {
        Self {
            aliases: CommandAliases::new(),
            registrations: HashMap::new(),
        }
    }

    /// The commands that ship with CadSlate2D.
    pub fn standard() -> Self {
        let mut registry = Self::empty();
        let standard: [(&'static str, &[&str], CommandFactory); 7] = [
            ("CIRCLE", &["C"], || Box::new(CircleCommand::new())),
            ("ARC", &["A"], || Box::new(ArcCommand::new())),
            ("LINE", &["L"], || Box::new(LineCommand::new())),
            ("UNDO", &["U"], || Box::new(UndoCommand)),
            ("REDO", &[], || Box::new(RedoCommand)),
            ("OPEN", &["DXFIN"], || Box::new(OpenCommand)),
            ("DXFOUT", &["SAVE", "SAVEAS", "EXPORT"], || {
                Box::new(ExportCommand)
            }),
        ];
        for (name, aliases, factory) in standard {
            registry
                .register(name, aliases, factory)
                .expect("standard command names are unique");
        }
        registry
    }

    /// Registers a command under a canonical name and aliases.
    pub fn register(
        &mut self,
        name: &'static str,
        aliases: &[&str],
        factory: CommandFactory,
    ) -> Result<(), DuplicateCommandName> {
        self.aliases.register(name, aliases)?;
        self.registrations
            .insert(name.to_owned(), Registration { name, factory });
        Ok(())
    }

    /// The table that maps typed text to canonical names.
    pub fn aliases(&self) -> &CommandAliases {
        &self.aliases
    }

    /// A fresh instance of the command with this canonical name.
    pub(crate) fn create(&self, canonical: &str) -> Option<(&'static str, Box<dyn Command>)> {
        self.registrations
            .get(canonical)
            .map(|registration| (registration.name, (registration.factory)()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_commands_are_reachable_by_name_and_alias() {
        let registry = CommandRegistry::standard();
        assert_eq!(registry.aliases().resolve("l"), Some("LINE"));
        let (name, _) = registry.create("LINE").unwrap();
        assert_eq!(name, "LINE");
    }

    #[test]
    fn unknown_commands_cannot_be_created() {
        assert!(CommandRegistry::standard().create("NOPE").is_none());
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = CommandRegistry::standard();
        let result = registry.register("LENGTHEN", &["L"], || Box::new(LineCommand::new()));
        assert_eq!(result, Err(DuplicateCommandName("L".to_owned())));
        assert!(registry.create("LENGTHEN").is_none());
    }
}
