//! Lineweights: how heavy a line is drawn and plotted (DESIGN §23.1, §32).
//!
//! A lineweight is one of a fixed list of widths in hundredths of a
//! millimetre — the list drafting standards and DXF (group 370) share — or
//! `Default`, the drawing's ordinary width, or a reference to the layer's
//! or the block's. It is a property of how an object looks, never of its
//! geometry: picking and snapping do not see it.

use crate::{Layer, Lineweight};

/// A lineweight with the indirections resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    /// The ordinary width: whatever the theme draws a plain line with.
    Default,
    /// This many hundredths of a millimetre.
    Hundredths(u16),
}

impl Lineweight {
    /// The widths a lineweight can have, in hundredths of a millimetre.
    pub const STANDARD: [u16; 24] = [
        0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158,
        200, 211,
    ];

    /// The lineweight of that many hundredths of a millimetre, if it is one
    /// of the standard widths.
    pub fn standard(hundredths: u16) -> Option<Self> {
        Self::STANDARD
            .contains(&hundredths)
            .then_some(Self::Hundredths(hundredths))
    }

    /// Reads a lineweight as it is typed: a standard width in millimetres
    /// (`0.25`, `.5`, `1`, `0.25mm`), `Default`, `ByLayer`, or `ByBlock`.
    /// Case does not count. `None` for anything else, a width that is not
    /// in the standard list included.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        for (name, lineweight) in [
            ("bylayer", Self::ByLayer),
            ("byblock", Self::ByBlock),
            ("default", Self::Default),
        ] {
            if text.eq_ignore_ascii_case(name) {
                return Some(lineweight);
            }
        }
        let number = text
            .strip_suffix("mm")
            .or_else(|| text.strip_suffix("MM"))
            .unwrap_or(text)
            .trim_end();
        // Plain decimals only: digits with at most one point among them.
        let plain = !number.is_empty()
            && number
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && number.bytes().filter(|byte| *byte == b'.').count() <= 1
            && number.bytes().any(|byte| byte.is_ascii_digit());
        if !plain {
            return None;
        }
        let hundredths = number.parse::<f64>().ok()? * 100.0;
        let rounded = hundredths.round();
        // 0.25 is exactly a standard width; 0.251 is not one at all.
        if (hundredths - rounded).abs() > 1e-9 || !(0.0..=f64::from(u16::MAX)).contains(&rounded) {
            return None;
        }
        Self::standard(rounded as u16)
    }

    /// The lineweight as it is shown: `ByLayer`, `Default`, `0.25 mm`.
    pub fn describe(self) -> String {
        match self {
            Self::ByLayer => "ByLayer".to_owned(),
            Self::ByBlock => "ByBlock".to_owned(),
            Self::Default => "Default".to_owned(),
            Self::Hundredths(hundredths) => {
                format!("{}.{:02} mm", hundredths / 100, hundredths % 100)
            }
        }
    }

    /// Whether this is a width in itself, not a reference to a layer's or a
    /// block's: what a layer's own lineweight must be.
    pub const fn is_concrete(self) -> bool {
        matches!(self, Self::Default | Self::Hundredths(_))
    }

    /// The weight of an object with this lineweight on `layer`, outside any
    /// block: `ByLayer` is the layer's; `ByBlock`, with no block reference
    /// to take it from, is the default — as is a layer whose own lineweight
    /// is an indirection (ADR-0013).
    pub const fn weight_on(self, layer: &Layer) -> Weight {
        match self {
            Self::Hundredths(hundredths) => Weight::Hundredths(hundredths),
            Self::Default | Self::ByBlock => Weight::Default,
            Self::ByLayer => match layer.lineweight {
                Self::Hundredths(hundredths) => Weight::Hundredths(hundredths),
                Self::Default | Self::ByLayer | Self::ByBlock => Weight::Default,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lineweights_are_read_in_millimetres_and_only_the_standard_ones() {
        let cases = [
            ("0.25", 25),
            ("0.25mm", 25),
            ("0.25 mm", 25),
            (".5", 50),
            ("0.5", 50),
            ("0.50", 50),
            ("1", 100),
            ("1.0", 100),
            ("2.11", 211),
            ("0", 0),
            ("0.00", 0),
            ("0.05", 5),
            (" 0.13 ", 13),
            ("1.58MM", 158),
        ];
        for (typed, hundredths) in cases {
            assert_eq!(
                Lineweight::parse(typed),
                Some(Lineweight::Hundredths(hundredths)),
                "{typed:?}"
            );
        }
        for (typed, lineweight) in [
            ("ByLayer", Lineweight::ByLayer),
            ("BYBLOCK", Lineweight::ByBlock),
            ("default", Lineweight::Default),
        ] {
            assert_eq!(Lineweight::parse(typed), Some(lineweight));
        }
        for typed in [
            "", "0.26", "0.251", "3", "25", "-0.25", "+0.25", "1e0", "0.25cm", "thick", "0..25",
            ".", "mm", "0.25 0.5",
        ] {
            assert_eq!(Lineweight::parse(typed), None, "{typed:?}");
        }
    }

    #[test]
    fn every_standard_lineweight_is_shown_in_millimetres_and_reads_back() {
        assert_eq!(Lineweight::Hundredths(25).describe(), "0.25 mm");
        assert_eq!(Lineweight::Hundredths(5).describe(), "0.05 mm");
        assert_eq!(Lineweight::Hundredths(100).describe(), "1.00 mm");
        assert_eq!(Lineweight::Hundredths(211).describe(), "2.11 mm");
        assert_eq!(Lineweight::Default.describe(), "Default");
        let every = Lineweight::STANDARD
            .into_iter()
            .map(Lineweight::Hundredths)
            .chain([
                Lineweight::ByLayer,
                Lineweight::ByBlock,
                Lineweight::Default,
            ]);
        for lineweight in every {
            assert_eq!(
                Lineweight::parse(&lineweight.describe()),
                Some(lineweight),
                "{lineweight:?}"
            );
        }
        assert_eq!(Lineweight::standard(25), Some(Lineweight::Hundredths(25)));
        assert_eq!(Lineweight::standard(26), None);
        assert!(Lineweight::STANDARD.is_sorted());
    }

    #[test]
    fn by_layer_takes_the_layers_weight_and_by_block_the_default() {
        let heavy = Layer {
            lineweight: Lineweight::Hundredths(100),
            ..Layer::named("heavy")
        };
        let plain = Layer::named("plain");
        assert_eq!(
            Lineweight::ByLayer.weight_on(&heavy),
            Weight::Hundredths(100)
        );
        assert_eq!(Lineweight::ByLayer.weight_on(&plain), Weight::Default);
        assert_eq!(
            Lineweight::Hundredths(25).weight_on(&heavy),
            Weight::Hundredths(25),
            "its own weight wins"
        );
        assert_eq!(Lineweight::Default.weight_on(&heavy), Weight::Default);
        assert_eq!(Lineweight::ByBlock.weight_on(&heavy), Weight::Default);
        assert!(Lineweight::Default.is_concrete() && Lineweight::Hundredths(0).is_concrete());
        assert!(!Lineweight::ByLayer.is_concrete() && !Lineweight::ByBlock.is_concrete());
    }
}
