//! Context-sensitive input resolution (DESIGN §13.3).
//!
//! The same text means different things at different prompts. The active
//! prompt states what it expects, and resolution is a pure function of
//! `(text, expectation, alias table)` — so it is deterministic by construction.

use crate::number::{Span, parse_number};
use crate::{CommandAliases, Diagnostic, Keyword, PointInput, parse_point};

/// The primary kind of input a prompt accepts.
///
/// Not `#[non_exhaustive]`: adding a kind in a later milestone must force
/// every match to be revisited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// The idle prompt: a command name or alias.
    Command,
    /// A point in any coordinate syntax.
    Point,
    /// A length: a number, or a point that the caller measures from a base point.
    Distance,
    /// An angle in degrees, counter-clockwise: a number, or a point that the
    /// caller measures from a base point.
    Angle,
    /// Nothing but an option keyword or Enter.
    Nothing,
}

/// What the current prompt accepts: one primary kind plus option keywords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedInput {
    pub kind: InputKind,
    /// Options offered in brackets. Inside a command they take precedence
    /// over every other reading of the text.
    pub keywords: Vec<Keyword>,
    /// At a point prompt: a bare number is a distance along the cursor
    /// direction from the previous point (direct-distance entry).
    pub direct_distance: bool,
}

impl ExpectedInput {
    pub fn new(kind: InputKind) -> Self {
        Self {
            kind,
            keywords: Vec::new(),
            direct_distance: false,
        }
    }

    pub fn command() -> Self {
        Self::new(InputKind::Command)
    }

    pub fn point() -> Self {
        Self::new(InputKind::Point)
    }

    pub fn distance() -> Self {
        Self::new(InputKind::Distance)
    }

    pub fn angle() -> Self {
        Self::new(InputKind::Angle)
    }

    pub fn with_keywords(mut self, keywords: &[Keyword]) -> Self {
        self.keywords.extend_from_slice(keywords);
        self
    }

    pub fn with_direct_distance(mut self) -> Self {
        self.direct_distance = true;
        self
    }
}

/// Typed input, ready for a command state machine.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// A canonical command name.
    Command(String),
    /// A point, not yet resolved against the previous point.
    Point(PointInput),
    /// A length in drawing units.
    Distance(f64),
    /// An angle in degrees, counter-clockwise from +X.
    Angle(f64),
    /// A distance to travel from the previous point towards the cursor.
    DirectDistance(f64),
    /// One of the prompt's option keywords.
    Keyword(Keyword),
    /// Enter or Space on an empty line: accept, confirm, or finish.
    Accept,
}

/// Interprets one line of command-line text for the given expectation.
///
/// Text that does not fit the expectation is a diagnostic — never a silent
/// fallback to some other interpretation.
pub fn resolve(
    text: &str,
    expected: &ExpectedInput,
    aliases: &CommandAliases,
) -> Result<Input, Diagnostic> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Input::Accept);
    }
    let position = text.len() - text.trim_start().len();

    if expected.kind == InputKind::Command {
        return match aliases.resolve(trimmed) {
            Some(canonical) => Ok(Input::Command(canonical.to_owned())),
            None => Err(Diagnostic::new(
                format!("Unknown command \"{trimmed}\"."),
                position,
            )),
        };
    }

    // Inside a command, an option keyword wins over any other reading (§13.3):
    // at `[Close/Undo]`, `C` is Close, not CIRCLE.
    let matching: Vec<Keyword> = expected
        .keywords
        .iter()
        .copied()
        .filter(|k| k.matches(trimmed))
        .collect();
    match matching.as_slice() {
        [] => {}
        [keyword] => return Ok(Input::Keyword(*keyword)),
        several => {
            let names: Vec<&str> = several.iter().map(|keyword| keyword.name()).collect();
            let message = format!("Ambiguous option \"{trimmed}\": {}?", names.join(" or "));
            return Err(Diagnostic::new(message, position));
        }
    }

    let number = || parse_number(Span::new(text, 0));
    let result = match expected.kind {
        InputKind::Command => unreachable!("handled above"),
        InputKind::Point => parse_point(text)
            .map(Input::Point)
            .or_else(|error| match number() {
                Ok(distance) if expected.direct_distance => Ok(Input::DirectDistance(distance)),
                _ => Err(error),
            }),
        // A number, or a point for the caller to measure from its base point.
        InputKind::Distance => number()
            .map(Input::Distance)
            .or_else(|_| parse_point(text).map(Input::Point)),
        InputKind::Angle => number()
            .map(Input::Angle)
            .or_else(|_| parse_point(text).map(Input::Point)),
        InputKind::Nothing => Err(Diagnostic::new("", position)),
    };
    result.map_err(|error| Diagnostic::new(requirement(expected), error.position.max(position)))
}

/// A one-line reminder of everything the prompt accepts.
fn requirement(expected: &ExpectedInput) -> String {
    let mut accepted: Vec<&str> = match expected.kind {
        InputKind::Command => vec!["a command"],
        InputKind::Point => vec!["a point"],
        InputKind::Distance => vec!["a distance", "a point"],
        InputKind::Angle => vec!["an angle", "a point"],
        InputKind::Nothing => vec![],
    };
    if !expected.keywords.is_empty() || accepted.is_empty() {
        accepted.push("an option keyword");
    }
    let list = match accepted.as_slice() {
        [only] => (*only).to_owned(),
        [first, second] => format!("{first} or {second}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
        [] => unreachable!("at least one alternative is always accepted"),
    };
    let mut sentence = format!("{list} is required.");
    sentence[..1].make_ascii_uppercase();
    sentence
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;

    const CLOSE: Keyword = Keyword::new("Close");
    const UNDO: Keyword = Keyword::new("Undo");

    fn aliases() -> CommandAliases {
        let mut aliases = CommandAliases::new();
        aliases.register("LINE", &["L"]).unwrap();
        aliases.register("CIRCLE", &["C"]).unwrap();
        aliases
    }

    fn resolved(text: &str, expected: &ExpectedInput) -> Result<Input, Diagnostic> {
        resolve(text, expected, &aliases())
    }

    fn message(text: &str, expected: &ExpectedInput) -> String {
        resolved(text, expected).unwrap_err().message
    }

    fn command(name: &str) -> Result<Input, Diagnostic> {
        Ok(Input::Command(name.to_owned()))
    }

    fn point(x: f64, y: f64) -> Result<Input, Diagnostic> {
        Ok(Input::Point(PointInput::Absolute(Point2::new(x, y))))
    }

    /// DESIGN §26.3 criterion 7: `L` and `LINE` invoke the same command.
    #[test]
    fn every_spelling_of_a_command_resolves_identically() {
        for text in ["L", "l", "LINE", "line", " Line "] {
            assert_eq!(
                resolved(text, &ExpectedInput::command()),
                command("LINE"),
                "{text:?}"
            );
        }
    }

    #[test]
    fn empty_input_is_accept_in_every_context() {
        let contexts = [
            ExpectedInput::command(),
            ExpectedInput::point(),
            ExpectedInput::distance().with_keywords(&[CLOSE]),
            ExpectedInput::angle(),
            ExpectedInput::new(InputKind::Nothing),
        ];
        for expected in contexts {
            assert_eq!(resolved("", &expected), Ok(Input::Accept));
            assert_eq!(resolved("   ", &expected), Ok(Input::Accept));
        }
    }

    /// DESIGN §13.3, verbatim: at `Specify next point or [Close/Undo]:`, `C`
    /// resolves to Close; at the idle prompt, `C` may resolve to CIRCLE.
    #[test]
    fn c_is_close_inside_line_and_circle_at_the_idle_prompt() {
        let next_point = ExpectedInput::point().with_keywords(&[CLOSE, UNDO]);
        assert_eq!(resolved("C", &next_point), Ok(Input::Keyword(CLOSE)));
        assert_eq!(resolved("c", &next_point), Ok(Input::Keyword(CLOSE)));
        assert_eq!(resolved("C", &ExpectedInput::command()), command("CIRCLE"));
    }

    /// The whole decision table for one prompt, so the rule is visible at a glance.
    #[test]
    fn resolution_table_for_a_point_prompt_with_options() {
        let expected = ExpectedInput::point()
            .with_keywords(&[CLOSE, UNDO])
            .with_direct_distance();
        let cases: [(&str, Result<Input, Diagnostic>); 9] = [
            ("10,20", point(10.0, 20.0)),
            ("C", Ok(Input::Keyword(CLOSE))),
            ("close", Ok(Input::Keyword(CLOSE))),
            ("U", Ok(Input::Keyword(UNDO))),
            ("und", Ok(Input::Keyword(UNDO))),
            ("500", Ok(Input::DirectDistance(500.0))),
            ("-2.5", Ok(Input::DirectDistance(-2.5))),
            ("", Ok(Input::Accept)),
            (
                "@5<90",
                Ok(Input::Point(PointInput::RelativePolar {
                    distance: 5.0,
                    angle_degrees: 90.0,
                })),
            ),
        ];
        for (text, outcome) in cases {
            assert_eq!(resolved(text, &expected), outcome, "{text:?}");
        }
        for text in ["L", "LINE", "CX", "banana", "1,2,3"] {
            assert_eq!(
                message(text, &expected),
                "A point or an option keyword is required.",
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_bare_number_at_a_point_prompt_needs_direct_distance_to_be_enabled() {
        assert_eq!(
            message("500", &ExpectedInput::point()),
            "A point is required."
        );
        assert_eq!(
            resolved("500", &ExpectedInput::point().with_direct_distance()),
            Ok(Input::DirectDistance(500.0))
        );
    }

    #[test]
    fn distance_and_angle_prompts_take_a_number_or_a_point() {
        assert_eq!(
            resolved("12.5", &ExpectedInput::distance()),
            Ok(Input::Distance(12.5))
        );
        assert_eq!(resolved("3,4", &ExpectedInput::distance()), point(3.0, 4.0));
        assert_eq!(
            resolved("-45", &ExpectedInput::angle()),
            Ok(Input::Angle(-45.0))
        );
        assert_eq!(resolved("0,1", &ExpectedInput::angle()), point(0.0, 1.0));
        assert_eq!(
            message("far", &ExpectedInput::distance()),
            "A distance or a point is required."
        );
        assert_eq!(
            message("north", &ExpectedInput::angle()),
            "An angle or a point is required."
        );
    }

    #[test]
    fn a_number_stays_a_number_next_to_a_digit_keyword() {
        let two_point = Keyword::new("2P");
        let expected =
            ExpectedInput::distance().with_keywords(&[two_point, Keyword::new("Diameter")]);
        assert_eq!(resolved("2", &expected), Ok(Input::Distance(2.0)));
        assert_eq!(resolved("2p", &expected), Ok(Input::Keyword(two_point)));
        assert_eq!(
            resolved("d", &expected),
            Ok(Input::Keyword(Keyword::new("Diameter")))
        );
        assert_eq!(
            message("x", &expected),
            "A distance, a point, or an option keyword is required."
        );
    }

    #[test]
    fn ambiguous_abbreviations_are_refused_not_guessed() {
        // Poorly chosen keywords: both start with a capital C.
        let expected = ExpectedInput::point().with_keywords(&[CLOSE, Keyword::new("Continue")]);
        assert_eq!(
            message("C", &expected),
            "Ambiguous option \"C\": Close or Continue?"
        );
        assert_eq!(resolved("CL", &expected), Ok(Input::Keyword(CLOSE)));
    }

    #[test]
    fn a_keyword_only_prompt_accepts_nothing_else() {
        let expected = ExpectedInput::new(InputKind::Nothing)
            .with_keywords(&[Keyword::new("Yes"), Keyword::new("No")]);
        assert_eq!(
            resolved("y", &expected),
            Ok(Input::Keyword(Keyword::new("Yes")))
        );
        assert_eq!(message("1,2", &expected), "An option keyword is required.");
        assert_eq!(
            message("maybe", &expected),
            "An option keyword is required."
        );
    }

    #[test]
    fn command_names_are_not_recognised_inside_a_command() {
        assert_eq!(
            message("LINE", &ExpectedInput::point()),
            "A point is required."
        );
    }

    #[test]
    fn unknown_commands_are_reported_by_name() {
        let error = resolved("  frobnicate", &ExpectedInput::command()).unwrap_err();
        assert_eq!(error.message, "Unknown command \"frobnicate\".");
        assert_eq!(error.position, 2);
    }

    #[test]
    fn diagnostics_keep_the_position_of_the_problem() {
        assert_eq!(
            resolved("5,b", &ExpectedInput::point())
                .unwrap_err()
                .position,
            2
        );
    }

    proptest! {
        #[test]
        fn resolution_is_deterministic_and_never_panics(text in "\\PC*") {
            let aliases = aliases();
            let contexts = [
                ExpectedInput::command(),
                ExpectedInput::point().with_keywords(&[CLOSE, UNDO]).with_direct_distance(),
                ExpectedInput::distance().with_keywords(&[Keyword::new("2P")]),
                ExpectedInput::angle(),
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[CLOSE]),
            ];
            for expected in &contexts {
                prop_assert_eq!(resolve(&text, expected, &aliases), resolve(&text, expected, &aliases));
            }
        }
    }
}
