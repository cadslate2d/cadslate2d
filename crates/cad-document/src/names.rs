//! The names of table rows — layers, linetypes, and later text styles and
//! blocks — and the one set of rules they all follow (DESIGN §11, §32).
//!
//! The rules are DXF's for symbol names, which are also what a command
//! line can take unambiguously: `,` separates names, `*` and `?` are
//! wildcards. Two names that differ only in case are the same name.

/// The longest name a row may have, in characters.
pub const MAX_NAME: usize = 255;

/// Characters a name may not contain.
pub const FORBIDDEN: &[char] = &[
    '<', '>', '/', '\\', '"', ':', ';', '?', '*', '|', '=', ',', '`',
];

/// Whether `name` can be a row's name.
pub fn check_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > MAX_NAME {
        return Err(NameError::TooLong);
    }
    if name != name.trim() {
        return Err(NameError::Padded);
    }
    match name
        .chars()
        .find(|c| c.is_control() || FORBIDDEN.contains(c))
    {
        Some(found) => Err(NameError::Forbidden(found)),
        None => Ok(()),
    }
}

/// Whether two names are the same name: case does not count.
pub fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Why a string cannot be a row's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// More than [`MAX_NAME`] characters.
    TooLong,
    /// It begins or ends with a space.
    Padded,
    /// It contains this character, which a name may not.
    Forbidden(char),
}

impl NameError {
    /// The refusal in the user's terms, for the kind of row named:
    /// `explain("layer")` gives "A layer name cannot be empty.".
    pub fn explain(self, kind: &str) -> String {
        match self {
            Self::Empty => format!("A {kind} name cannot be empty."),
            Self::TooLong => {
                format!("A {kind} name can have at most {MAX_NAME} characters.")
            }
            Self::Padded => format!("A {kind} name cannot begin or end with a space."),
            Self::Forbidden(c) if c.is_control() => {
                format!("A {kind} name cannot contain control characters.")
            }
            Self::Forbidden(c) => format!("A {kind} name cannot contain \"{c}\"."),
        }
    }
}

/// A row that has a name, so its table can be searched by it.
pub trait Named {
    fn name(&self) -> &str;
}

impl<Row: Named> crate::Table<Row> {
    /// The row of that name, whatever its case.
    pub fn find(&self, name: &str) -> Option<crate::TableId<Row>> {
        self.iter()
            .find(|(_, row)| same_name(row.name(), name))
            .map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked_against_what_a_file_and_a_command_line_can_hold() {
        for good in [
            "0",
            "walls",
            "Ground floor",
            "A-WALL-EXT",
            "Größe 12",
            "x".repeat(255).as_str(),
        ] {
            assert_eq!(check_name(good), Ok(()), "{good}");
        }
        assert_eq!(check_name(""), Err(NameError::Empty));
        assert_eq!(check_name(&"x".repeat(256)), Err(NameError::TooLong));
        for padded in [" walls", "walls ", " "] {
            assert_eq!(check_name(padded), Err(NameError::Padded), "{padded:?}");
        }
        for forbidden in FORBIDDEN {
            let name = format!("a{forbidden}b");
            assert_eq!(check_name(&name), Err(NameError::Forbidden(*forbidden)));
        }
        assert_eq!(check_name("a\tb"), Err(NameError::Forbidden('\t')));
    }

    #[test]
    fn every_name_error_explains_itself_for_the_kind_of_row() {
        let messages = [
            NameError::Empty.explain("layer"),
            NameError::TooLong.explain("layer"),
            NameError::Padded.explain("layer"),
            NameError::Forbidden('*').explain("linetype"),
            NameError::Forbidden('\n').explain("layer"),
        ];
        assert_eq!(messages[3], "A linetype name cannot contain \"*\".");
        assert_eq!(messages[1], "A layer name can have at most 255 characters.");
        assert!(messages.iter().all(|message| message.ends_with('.')));
    }

    #[test]
    fn case_does_not_tell_names_apart() {
        assert!(same_name("STRAßE", "straße"));
        assert!(same_name("Walls", "WALLS"));
        assert!(!same_name("walls", "wall"));
    }
}
