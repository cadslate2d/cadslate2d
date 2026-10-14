//! Dimension styles: the table a dimension is drawn by (DESIGN §19.1, §34).
//!
//! A dimension style says how a dimension looks — how tall its text is,
//! how big its arrowheads, how far its extension lines stand off the
//! points they measure and run past the dimension line, how many decimals
//! its value has and what follows it. `Standard` is the table's permanent
//! first row; every dimension refers to a style, as every text refers to a
//! text style, and the one the drawing is set to is what new dimensions
//! take. Changing a style is one row changing, however many dimensions it
//! draws (§19.6).

use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A dimension style. Lengths are in drawing units.
#[derive(Clone, Debug, PartialEq)]
pub struct DimStyle {
    pub name: String,
    /// The height of the dimension's text. Positive.
    pub text_height: f64,
    /// The length of an arrowhead. Positive.
    pub arrow_size: f64,
    /// The gap between the dimension line and the text. Not negative.
    pub text_gap: f64,
    /// How far an extension line starts from the point it measures. Not
    /// negative.
    pub extension_offset: f64,
    /// How far an extension line runs past the dimension line. Not negative.
    pub extension_overshoot: f64,
    /// How far apart the dimension lines of a baseline chain stand
    /// (`DIMDLI`, #212). Not negative.
    pub baseline_spacing: f64,
    /// Decimal places of the measured value, 0 to 8.
    pub decimals: u8,
    /// What follows the value: a unit, a note; nothing by default.
    pub suffix: String,
}

/// Why a style cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimStyleError {
    /// The named length must be a positive number.
    NotPositive(&'static str),
    /// The named length must not be negative.
    Negative(&'static str),
    /// More decimals than a value can show.
    TooManyDecimals,
}

impl DimStyleError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> String {
        match self {
            Self::NotPositive(what) => format!("The {what} must be greater than zero."),
            Self::Negative(what) => format!("The {what} cannot be negative."),
            Self::TooManyDecimals => format!(
                "A dimension shows at most {} decimals.",
                DimStyle::MAX_DECIMALS
            ),
        }
    }
}

impl DimStyle {
    /// The name of the style every drawing has.
    pub const STANDARD: &'static str = "Standard";
    /// The most decimal places a value is shown to.
    pub const MAX_DECIMALS: u8 = 8;

    /// A style with the classic defaults: text 2.5 high, arrowheads as long,
    /// a gap and an offset of a quarter of that, overshoot of half, two
    /// decimals, nothing after the value.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text_height: 2.5,
            arrow_size: 2.5,
            text_gap: 0.625,
            extension_offset: 0.625,
            extension_overshoot: 1.25,
            baseline_spacing: 3.75,
            decimals: 2,
            suffix: String::new(),
        }
    }

    /// The same style under another name.
    #[must_use]
    pub fn renamed(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..self.clone()
        }
    }

    /// Whether the style's values are ones a dimension can be drawn by.
    /// The name is checked where the table is.
    pub fn check(&self) -> Result<(), DimStyleError> {
        let positive = |value: f64, what| {
            (value.is_finite() && value > 0.0)
                .then_some(())
                .ok_or(DimStyleError::NotPositive(what))
        };
        let not_negative = |value: f64, what| {
            (value.is_finite() && value >= 0.0)
                .then_some(())
                .ok_or(DimStyleError::Negative(what))
        };
        positive(self.text_height, "text height")?;
        positive(self.arrow_size, "arrow size")?;
        not_negative(self.text_gap, "text gap")?;
        not_negative(self.extension_offset, "extension line offset")?;
        not_negative(self.extension_overshoot, "extension line overshoot")?;
        not_negative(self.baseline_spacing, "baseline spacing")?;
        if self.decimals > Self::MAX_DECIMALS {
            return Err(DimStyleError::TooManyDecimals);
        }
        Ok(())
    }

    /// A measured value as the style shows it: rounded to its decimals,
    /// with its suffix. Negative zero is zero.
    pub fn format(&self, value: f64) -> String {
        // A half rounds away from zero, as a drafter rounds, not to even.
        let factor = 10f64.powi(i32::from(self.decimals));
        let mut rounded = (value * factor).round() / factor;
        if rounded == 0.0 {
            rounded = 0.0; // never "-0"
        }
        format!("{rounded:.*}{}", usize::from(self.decimals), self.suffix)
    }
}

impl Named for DimStyle {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type DimStyleId = TableId<DimStyle>;
pub type DimStyleTable = Table<DimStyle>;
/// A reversible change to one dimension style: made, removed, or — its
/// values or its name — replaced by one row.
pub type DimStyleChange = TableChange<DimStyle>;

/// A new drawing's dimension styles: just `Standard`.
pub(crate) fn initial_dim_styles() -> DimStyleTable {
    Table::starting_with(DimStyle::named(DimStyle::STANDARD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_drawing_has_standard_with_the_classic_defaults() {
        let table = initial_dim_styles();
        assert_eq!(table.len(), 1);
        let standard = table.get(DimStyleId::FIRST).unwrap();
        assert_eq!(standard.name, "Standard");
        assert_eq!(table.find("STANDARD"), Some(DimStyleId::FIRST));
        assert_eq!((standard.text_height, standard.arrow_size), (2.5, 2.5));
        assert_eq!(standard.decimals, 2);
        assert_eq!(standard.baseline_spacing, 3.75);
        assert!(standard.check().is_ok());
    }

    #[test]
    fn a_style_refuses_what_a_dimension_cannot_be_drawn_by() {
        let mut style = DimStyle::named("Arch");
        style.text_height = 0.0;
        assert_eq!(
            style.check(),
            Err(DimStyleError::NotPositive("text height"))
        );
        style.text_height = f64::NAN;
        assert_eq!(
            style.check(),
            Err(DimStyleError::NotPositive("text height"))
        );
        style = DimStyle::named("Arch");
        style.arrow_size = -1.0;
        assert_eq!(style.check(), Err(DimStyleError::NotPositive("arrow size")));
        style = DimStyle::named("Arch");
        style.text_gap = -0.1;
        assert_eq!(style.check(), Err(DimStyleError::Negative("text gap")));
        style = DimStyle::named("Arch");
        style.extension_offset = f64::INFINITY;
        assert_eq!(
            style.check(),
            Err(DimStyleError::Negative("extension line offset"))
        );
        style = DimStyle::named("Arch");
        style.baseline_spacing = -1.0;
        assert_eq!(
            style.check(),
            Err(DimStyleError::Negative("baseline spacing"))
        );
        style = DimStyle::named("Arch");
        style.decimals = 9;
        assert_eq!(style.check(), Err(DimStyleError::TooManyDecimals));
        assert_eq!(
            DimStyleError::TooManyDecimals.explain(),
            "A dimension shows at most 8 decimals."
        );
        style.decimals = 0;
        style.text_gap = 0.0;
        assert!(
            style.check().is_ok(),
            "zero gap and no decimals are allowed"
        );
    }

    #[test]
    fn a_value_is_shown_to_the_decimals_with_the_suffix() {
        let mut style = DimStyle::named("Arch");
        assert_eq!(style.format(12.345), "12.35");
        assert_eq!(style.format(-0.0001), "0.00", "negative zero is zero");
        style.decimals = 0;
        style.suffix = " mm".to_owned();
        assert_eq!(style.format(12.5), "13 mm");
        assert_eq!(style.format(-3.2), "-3 mm");
        assert_eq!(style.renamed("Civil").name, "Civil");
        assert_eq!(style.renamed("Civil").suffix, " mm");
    }
}
