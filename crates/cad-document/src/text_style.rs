//! Text styles: the third of the drawing's tables (DESIGN §11, §32).
//!
//! A text style names how a text is drawn. With one font there is nothing
//! to choose yet, so a style is its name; `Standard` is the table's
//! permanent first row, and every text refers to a style, as every entity
//! refers to a layer. The row grows a font, a width factor, and an oblique
//! angle when there is more than one of each to choose from.

use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A text style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub name: String,
}

impl TextStyle {
    /// The name of the style every drawing has.
    pub const STANDARD: &'static str = "Standard";

    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Named for TextStyle {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type TextStyleId = TableId<TextStyle>;
pub type TextStyleTable = Table<TextStyle>;
/// A reversible change to one text style: made, removed, or renamed.
pub type TextStyleChange = TableChange<TextStyle>;

/// A new drawing's text styles: just `Standard`.
pub(crate) fn initial_text_styles() -> TextStyleTable {
    Table::starting_with(TextStyle::named(TextStyle::STANDARD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_drawing_has_standard_and_finds_it_whatever_the_case() {
        let table = initial_text_styles();
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(TextStyleId::FIRST).unwrap().name, "Standard");
        assert_eq!(table.find("STANDARD"), Some(TextStyleId::FIRST));
        assert_eq!(table.find("Roman"), None);
    }
}
