//! Context-sensitive input resolution (DESIGN §13.3).
//!
//! The same text means different things at different prompts. The active
//! prompt states what it expects, and resolution is a pure function of
//! `(text, expectation, alias table)` — so it is deterministic by construction.

use crate::{CommandAliases, Diagnostic, PointInput, parse_point};

/// What the current prompt accepts.
///
/// Not `#[non_exhaustive]`: adding an expectation in a later milestone must
/// force every match to be revisited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedInput {
    /// The idle prompt: a command name or alias.
    Command,
    /// A point in any coordinate syntax.
    Point,
}

/// Typed input, ready for a command state machine.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// A canonical command name.
    Command(String),
    /// A point, not yet resolved against the previous point.
    Point(PointInput),
    /// Enter or Space on an empty line: accept, confirm, or finish.
    Accept,
}

/// Interprets one line of command-line text for the given expectation.
///
/// Text that does not fit the expectation is a diagnostic — never a silent
/// fallback to some other interpretation.
pub fn resolve(
    text: &str,
    expected: ExpectedInput,
    aliases: &CommandAliases,
) -> Result<Input, Diagnostic> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Input::Accept);
    }
    match expected {
        ExpectedInput::Command => match aliases.resolve(trimmed) {
            Some(canonical) => Ok(Input::Command(canonical.to_owned())),
            None => {
                let position = text.len() - text.trim_start().len();
                Err(Diagnostic::new(
                    format!("Unknown command \"{trimmed}\"."),
                    position,
                ))
            }
        },
        ExpectedInput::Point => parse_point(text).map(Input::Point),
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;

    fn aliases() -> CommandAliases {
        let mut aliases = CommandAliases::new();
        aliases.register("LINE", &["L"]).unwrap();
        aliases
    }

    fn line() -> Result<Input, Diagnostic> {
        Ok(Input::Command("LINE".to_owned()))
    }

    /// DESIGN §26.3 criterion 7: `L` and `LINE` invoke the same command.
    #[test]
    fn every_spelling_of_a_command_resolves_identically() {
        for text in ["L", "l", "LINE", "line", " Line "] {
            assert_eq!(
                resolve(text, ExpectedInput::Command, &aliases()),
                line(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn empty_input_is_accept_in_every_context() {
        for expected in [ExpectedInput::Command, ExpectedInput::Point] {
            assert_eq!(resolve("", expected, &aliases()), Ok(Input::Accept));
            assert_eq!(resolve("   ", expected, &aliases()), Ok(Input::Accept));
        }
    }

    #[test]
    fn the_same_text_resolves_by_context() {
        let aliases = aliases();
        // At the idle prompt `L` is a command; at a point prompt it is an error.
        assert_eq!(resolve("L", ExpectedInput::Command, &aliases), line());
        assert!(resolve("L", ExpectedInput::Point, &aliases).is_err());
        // At a point prompt `1,2` is a point; at the idle prompt it is an error.
        assert_eq!(
            resolve("1,2", ExpectedInput::Point, &aliases),
            Ok(Input::Point(PointInput::Absolute(Point2::new(1.0, 2.0))))
        );
        assert!(resolve("1,2", ExpectedInput::Command, &aliases).is_err());
    }

    #[test]
    fn unknown_commands_are_reported_by_name() {
        let error = resolve("  frobnicate", ExpectedInput::Command, &aliases()).unwrap_err();
        assert_eq!(error.message, "Unknown command \"frobnicate\".");
        assert_eq!(error.position, 2);
    }

    proptest! {
        #[test]
        fn resolution_is_deterministic_and_never_panics(text in "\\PC*") {
            let aliases = aliases();
            for expected in [ExpectedInput::Command, ExpectedInput::Point] {
                prop_assert_eq!(resolve(&text, expected, &aliases), resolve(&text, expected, &aliases));
            }
        }
    }
}
