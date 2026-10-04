//! Command names and aliases.
//!
//! The table is data supplied by the caller. This crate knows the *shape* of
//! command naming — case-insensitive names with any number of aliases — but
//! not which commands exist (ADR-0001).

use std::collections::HashMap;
use std::fmt;

/// Returned when a name or alias is registered twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateCommandName(pub String);

impl fmt::Display for DuplicateCommandName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "command name or alias \"{}\" is already registered",
            self.0
        )
    }
}

impl std::error::Error for DuplicateCommandName {}

/// Maps what the user types to canonical command names.
#[derive(Clone, Debug, Default)]
pub struct CommandAliases {
    canonical_by_name: HashMap<String, String>,
}

impl CommandAliases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a command under its canonical name and any aliases.
    /// Nothing is registered if any of the names is already taken.
    pub fn register(
        &mut self,
        canonical: &str,
        aliases: &[&str],
    ) -> Result<(), DuplicateCommandName> {
        let names: Vec<String> = std::iter::once(canonical)
            .chain(aliases.iter().copied())
            .map(normalize)
            .collect();
        if let Some(taken) = names
            .iter()
            .find(|name| self.canonical_by_name.contains_key(*name))
        {
            return Err(DuplicateCommandName(taken.clone()));
        }
        let canonical = normalize(canonical);
        for name in names {
            self.canonical_by_name.insert(name, canonical.clone());
        }
        Ok(())
    }

    /// The canonical command for `text`, ignoring case and surrounding space.
    pub fn resolve(&self, text: &str) -> Option<&str> {
        self.canonical_by_name
            .get(&normalize(text))
            .map(String::as_str)
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases() -> CommandAliases {
        let mut aliases = CommandAliases::new();
        aliases.register("LINE", &["L"]).unwrap();
        aliases.register("UNDO", &["U"]).unwrap();
        aliases
    }

    #[test]
    fn names_and_aliases_resolve_to_the_canonical_name() {
        let aliases = aliases();
        for text in ["LINE", "line", "Line", "L", "l", "  l  "] {
            assert_eq!(aliases.resolve(text), Some("LINE"), "{text:?}");
        }
        assert_eq!(aliases.resolve("u"), Some("UNDO"));
    }

    #[test]
    fn unknown_names_do_not_resolve() {
        let aliases = aliases();
        assert_eq!(aliases.resolve("CIRCLE"), None);
        assert_eq!(aliases.resolve(""), None);
        assert_eq!(aliases.resolve("LI"), None);
    }

    #[test]
    fn duplicate_names_are_rejected_without_partial_registration() {
        let mut aliases = aliases();
        assert_eq!(
            aliases.register("LENGTHEN", &["LEN", "l"]),
            Err(DuplicateCommandName("L".to_owned()))
        );
        assert_eq!(aliases.resolve("LENGTHEN"), None);
        assert_eq!(aliases.resolve("LEN"), None);
        assert_eq!(aliases.resolve("L"), Some("LINE"));
    }
}
