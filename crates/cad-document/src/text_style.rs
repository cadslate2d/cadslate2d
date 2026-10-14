//! Text styles: the third of the drawing's tables (DESIGN §11, §32).
//!
//! A text style names how a text is drawn. With one font there is no font
//! to choose, so a style is its name and what a new text takes from it
//! (#221): a fixed height, or none; a width factor; an oblique angle.
//! A single-line text keeps its own width and slant once made, as DXF's
//! TEXT does, so changing a style changes no text already drawn.
//! `Standard` is the table's permanent first row, and every text refers
//! to a style, as every entity refers to a layer.

use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A text style.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub name: String,
    /// The height its texts are made at, in drawing units; zero asks for
    /// one each time.
    pub height: f64,
    /// How wide its glyphs are made, against the font's own: one as drawn.
    pub width_factor: f64,
    /// How far its glyphs lean from upright, radians, positive to the
    /// right.
    pub oblique: f64,
}

/// Why a text style's values cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyleError {
    /// The height is negative or not a number.
    Height,
    /// The width factor is not between a hundredth and a hundred.
    WidthFactor,
    /// The oblique angle leans more than 85° either way.
    Oblique,
}

impl TextStyleError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> &'static str {
        match self {
            Self::Height => "The height must be zero, to ask each time, or more.",
            Self::WidthFactor => "The width factor must be between 0.01 and 100.",
            Self::Oblique => "The oblique angle must be between -85° and 85°.",
        }
    }
}

impl TextStyle {
    /// The name of the style every drawing has.
    pub const STANDARD: &'static str = "Standard";
    /// The most a glyph may lean, in radians: 85°.
    pub const MAX_OBLIQUE: f64 = 85.0 * std::f64::consts::PI / 180.0;

    /// A style with no fixed height, the font's own width, and no slant.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            height: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
        }
    }

    /// Whether the values are ones a text can be drawn by.
    pub fn check(&self) -> Result<(), TextStyleError> {
        if !(self.height.is_finite() && self.height >= 0.0) {
            return Err(TextStyleError::Height);
        }
        if !(self.width_factor.is_finite() && (0.01..=100.0).contains(&self.width_factor)) {
            return Err(TextStyleError::WidthFactor);
        }
        if !(self.oblique.is_finite() && self.oblique.abs() <= Self::MAX_OBLIQUE) {
            return Err(TextStyleError::Oblique);
        }
        Ok(())
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

    #[test]
    fn a_style_refuses_values_no_text_can_be_drawn_by() {
        let fine = TextStyle {
            height: 2.5,
            width_factor: 0.8,
            oblique: 15f64.to_radians(),
            ..TextStyle::named("Notes")
        };
        assert_eq!(fine.check(), Ok(()));
        let with = |change: fn(&mut TextStyle)| {
            let mut style = fine.clone();
            change(&mut style);
            style.check()
        };
        assert_eq!(with(|s| s.height = -1.0), Err(TextStyleError::Height));
        assert_eq!(
            with(|s| s.width_factor = 0.0),
            Err(TextStyleError::WidthFactor)
        );
        assert_eq!(
            with(|s| s.width_factor = f64::NAN),
            Err(TextStyleError::WidthFactor)
        );
        assert_eq!(with(|s| s.oblique = 1.6), Err(TextStyleError::Oblique));
        assert_eq!(with(|s| s.height = 0.0), Ok(()), "zero asks each time");
    }
}
