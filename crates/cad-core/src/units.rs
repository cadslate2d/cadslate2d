//! Units of length (DESIGN §14).
//!
//! Geometry is unit-independent: a coordinate is a number, and nothing in
//! the geometry knows what it is a number of. A drawing may *say* what its
//! numbers mean — its drawing unit — and then a typed length with a suffix,
//! `25mm`, can be converted into it. A drawing with no unit is unitless,
//! and takes plain numbers only.

/// A unit of length a drawing can be in, or a typed length can be given in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    Millimetres,
    Centimetres,
    Metres,
    Inches,
    Feet,
}

impl LengthUnit {
    /// Every unit, from the smallest metric one up, then the imperial ones.
    pub const ALL: [Self; 5] = [
        Self::Millimetres,
        Self::Centimetres,
        Self::Metres,
        Self::Inches,
        Self::Feet,
    ];

    /// How many metres one of this unit is. Exact by definition: the inch
    /// is 25.4 mm.
    pub const fn metres(self) -> f64 {
        match self {
            Self::Millimetres => 0.001,
            Self::Centimetres => 0.01,
            Self::Metres => 1.0,
            Self::Inches => 0.0254,
            Self::Feet => 0.3048,
        }
    }

    /// What is typed after a number to give it in this unit: `25mm`.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Millimetres => "mm",
            Self::Centimetres => "cm",
            Self::Metres => "m",
            Self::Inches => "in",
            Self::Feet => "ft",
        }
    }

    /// The unit's name, in the plural, as in "the drawing is in millimetres".
    pub const fn name(self) -> &'static str {
        match self {
            Self::Millimetres => "millimetres",
            Self::Centimetres => "centimetres",
            Self::Metres => "metres",
            Self::Inches => "inches",
            Self::Feet => "feet",
        }
    }

    /// The unit with this suffix, whatever its case.
    pub fn from_suffix(suffix: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|unit| unit.suffix().eq_ignore_ascii_case(suffix))
    }

    /// `length` of this unit, expressed in `target` units.
    ///
    /// Converting to the same unit gives the number back exactly, and whole
    /// ratios (metres to millimetres) are exact where the arithmetic allows.
    pub fn convert(self, length: f64, target: Self) -> f64 {
        if self == target {
            return length;
        }
        // Thousandths of a millimetre are whole numbers for every unit
        // here, so the ratio is a ratio of integers, not of 0.001 and 0.0254.
        let (from, to) = (self.micrometres(), target.micrometres());
        if from >= to && from % to == 0 {
            length * (from / to) as f64
        } else if to % from == 0 {
            length / (to / from) as f64
        } else {
            length * from as f64 / to as f64
        }
    }

    const fn micrometres(self) -> u64 {
        match self {
            Self::Millimetres => 1_000,
            Self::Centimetres => 10_000,
            Self::Metres => 1_000_000,
            Self::Inches => 25_400,
            Self::Feet => 304_800,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_are_recognised_whatever_their_case() {
        for unit in LengthUnit::ALL {
            assert_eq!(LengthUnit::from_suffix(unit.suffix()), Some(unit));
            assert_eq!(
                LengthUnit::from_suffix(&unit.suffix().to_uppercase()),
                Some(unit)
            );
        }
        for unknown in ["", "km", "mi", "\"", "'", "meter", "mmm"] {
            assert_eq!(LengthUnit::from_suffix(unknown), None, "{unknown:?}");
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn conversions_between_whole_ratios_are_exact() {
        use LengthUnit::{Centimetres, Feet, Inches, Metres, Millimetres};
        assert_eq!(Millimetres.convert(25.0, Metres), 0.025);
        assert_eq!(Metres.convert(0.025, Millimetres), 25.0);
        assert_eq!(Metres.convert(3.0, Centimetres), 300.0);
        assert_eq!(Inches.convert(1.0, Millimetres), 25.4);
        assert_eq!(Feet.convert(1.0, Inches), 12.0);
        assert_eq!(Inches.convert(6.0, Feet), 0.5);
        assert_eq!(Feet.convert(1.0, Millimetres), 304.8);
        for unit in LengthUnit::ALL {
            assert_eq!(
                unit.convert(0.1 + 0.2, unit),
                0.1 + 0.2,
                "{unit:?} to itself"
            );
        }
    }

    #[test]
    fn converting_there_and_back_gives_the_length_back() {
        for from in LengthUnit::ALL {
            for to in LengthUnit::ALL {
                for length in [1.0, 0.3, 1234.5678, -42.0, 1e-6, 1e9] {
                    let back = to.convert(from.convert(length, to), from);
                    assert!(
                        (back - length).abs() <= length.abs() * 4.0 * f64::EPSILON,
                        "{length} {from:?} → {to:?} → {back}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_metre_values_agree_with_the_ratios() {
        for unit in LengthUnit::ALL {
            let metres = unit.convert(1.0, LengthUnit::Metres);
            assert!((metres - unit.metres()).abs() < 1e-15, "{unit:?}");
            assert!(!unit.name().is_empty());
        }
    }
}
