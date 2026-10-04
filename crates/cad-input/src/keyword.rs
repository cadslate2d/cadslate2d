//! Option keywords, as in `Specify next point or [Close/Undo]:`.

use std::fmt;

/// One option keyword.
///
/// The name is written the way it is shown in a prompt. Its leading run of
/// capitals and digits is the shortest abbreviation the user may type —
/// `C` for `Close`, `2P` for `2P` — as in classic CAD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keyword(&'static str);

impl Keyword {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn name(self) -> &'static str {
        self.0
    }

    /// The shortest text that selects this keyword.
    fn abbreviation(self) -> &'static str {
        let end = self
            .0
            .find(|character: char| !(character.is_ascii_uppercase() || character.is_ascii_digit()))
            .unwrap_or(self.0.len());
        // A name with no leading capital has no abbreviation: type it in full.
        if end == 0 { self.0 } else { &self.0[..end] }
    }

    /// True when `text` selects this keyword: at least the abbreviation, and
    /// otherwise a prefix of the full name. Case does not matter.
    pub fn matches(self, text: &str) -> bool {
        let text = text.trim();
        text.len() >= self.abbreviation().len()
            && text.len() <= self.0.len()
            && self.0.as_bytes()[..text.len()].eq_ignore_ascii_case(text.as_bytes())
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capital_letter_is_enough() {
        let close = Keyword::new("Close");
        for text in ["C", "c", "CL", "clo", "CLOSE", "close", " c "] {
            assert!(close.matches(text), "{text:?}");
        }
    }

    #[test]
    fn text_that_is_not_a_prefix_does_not_match() {
        let close = Keyword::new("Close");
        for text in ["", "X", "CX", "CLOSED", "lose", "1,2"] {
            assert!(!close.matches(text), "{text:?}");
        }
    }

    #[test]
    fn longer_abbreviations_must_be_typed_in_full() {
        let two_point = Keyword::new("2P");
        assert!(two_point.matches("2p"));
        assert!(
            !two_point.matches("2"),
            "a bare 2 must stay available as a number"
        );
        assert!(!two_point.matches("2PX"));

        let diameter = Keyword::new("Diameter");
        assert!(diameter.matches("d") && diameter.matches("DIA"));
    }

    #[test]
    fn a_name_without_capitals_must_be_typed_in_full() {
        let odd = Keyword::new("exit");
        assert!(odd.matches("EXIT"));
        assert!(!odd.matches("e"));
    }

    #[test]
    fn displays_as_its_name() {
        assert_eq!(Keyword::new("Undo").to_string(), "Undo");
    }
}
