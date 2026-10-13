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

/// Maps what the user types to canonical command names: the built-in table,
/// and the user's own aliases over it (#186), which may not take a built-in
/// name.
#[derive(Clone, Debug, Default)]
pub struct CommandAliases {
    canonical_by_name: HashMap<String, String>,
    user_by_name: HashMap<String, String>,
}

impl CommandAliases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the user's aliases with those in `text`: one per line as
    /// `alias, command` (the command may carry acad.pgp's `*`), with `;`
    /// starting a comment. A line that cannot be taken is reported by its
    /// number and skipped; the rest stand. Returns the refusals.
    pub fn set_user_aliases(&mut self, text: &str) -> Vec<String> {
        let mut user = HashMap::new();
        let mut refused = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let number = index + 1;
            let Some((alias, command)) = line.split_once(',') else {
                refused.push(format!("Line {number} is not \"alias, command\": {line}"));
                continue;
            };
            let alias = normalize(alias);
            let command = normalize(command.trim().trim_start_matches('*'));
            if alias.is_empty() || alias.contains(char::is_whitespace) {
                refused.push(format!("Line {number}: an alias is one word: {line}"));
            } else if self.canonical_by_name.contains_key(&alias) {
                refused.push(format!(
                    "Line {number}: \"{alias}\" is a built-in command name or alias."
                ));
            } else {
                match (user.get(&alias), self.canonical_by_name.get(&command)) {
                    (Some(_), _) => refused.push(format!(
                        "Line {number}: \"{alias}\" is already defined above."
                    )),
                    (None, Some(canonical)) => {
                        user.insert(alias, canonical.clone());
                    }
                    (None, None) => {
                        refused.push(format!("Line {number}: there is no command \"{command}\"."))
                    }
                }
            }
        }
        self.user_by_name = user;
        refused
    }

    /// The user's aliases, one per line as `alias, command`, in name order:
    /// what the editor opens with and what the host keeps.
    pub fn user_aliases(&self) -> String {
        let mut entries: Vec<(&String, &String)> = self.user_by_name.iter().collect();
        entries.sort();
        entries
            .iter()
            .map(|(alias, command)| format!("{alias}, {command}\n"))
            .collect()
    }

    pub fn user_alias_count(&self) -> usize {
        self.user_by_name.len()
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
        let name = normalize(text);
        self.canonical_by_name
            .get(&name)
            .or_else(|| self.user_by_name.get(&name))
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

    #[test]
    fn user_aliases_resolve_and_are_rendered_in_order() {
        let mut aliases = aliases();
        let refused = aliases.set_user_aliases("; mine\nLL, line ; via the command\n  uu ,*U\n\n");
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(aliases.resolve("ll"), Some("LINE"));
        assert_eq!(
            aliases.resolve("UU"),
            Some("UNDO"),
            "through a built-in alias"
        );
        assert_eq!(aliases.user_aliases(), "LL, LINE\nUU, UNDO\n");
        assert_eq!(aliases.user_alias_count(), 2);
        // Setting again replaces: the old aliases are gone.
        assert!(aliases.set_user_aliases("X, LINE").is_empty());
        assert_eq!(aliases.resolve("LL"), None);
        assert_eq!(aliases.resolve("X"), Some("LINE"));
    }

    #[test]
    fn user_aliases_that_cannot_be_taken_are_refused_by_line_and_the_rest_stand() {
        let mut aliases = aliases();
        let refused = aliases.set_user_aliases(
            "L, UNDO\nLINE, UNDO\nZZ, NOPE\nbad line\nA B, LINE\nOK, LINE\nOK, UNDO\n",
        );
        assert_eq!(
            refused,
            vec![
                "Line 1: \"L\" is a built-in command name or alias.",
                "Line 2: \"LINE\" is a built-in command name or alias.",
                "Line 3: there is no command \"NOPE\".",
                "Line 4 is not \"alias, command\": bad line",
                "Line 5: an alias is one word: A B, LINE",
                "Line 7: \"OK\" is already defined above.",
            ]
        );
        assert_eq!(aliases.resolve("OK"), Some("LINE"));
        assert_eq!(
            aliases.resolve("L"),
            Some("LINE"),
            "the built-in alias is untouched"
        );
    }
}
