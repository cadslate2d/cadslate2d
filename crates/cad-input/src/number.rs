//! Numeric literals.

use cad_core::LengthUnit;

use crate::Diagnostic;

/// A slice of the input together with its byte offset, so diagnostics can
/// point at the right place.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Span<'a> {
    pub text: &'a str,
    pub offset: usize,
}

impl<'a> Span<'a> {
    pub(crate) fn new(text: &'a str, offset: usize) -> Self {
        Self { text, offset }
    }

    /// The span without surrounding whitespace.
    pub(crate) fn trimmed(self) -> Self {
        let without_leading = self.text.trim_start();
        let offset = self.offset + (self.text.len() - without_leading.len());
        Self::new(without_leading.trim_end(), offset)
    }

    /// The part of the span from byte `start` to byte `end` of its text.
    pub(crate) fn slice(self, start: usize, end: usize) -> Self {
        Self::new(&self.text[start..end], self.offset + start)
    }
}

/// Parses a finite decimal number: optional sign, digits with an optional
/// fraction (or a leading-dot fraction), and an optional exponent.
///
/// Stricter than `f64::from_str` on purpose: words like `inf` and `nan` are
/// not coordinates.
pub(crate) fn parse_number(span: Span<'_>) -> Result<f64, Diagnostic> {
    let span = span.trimmed();
    if span.text.is_empty() {
        return Err(Diagnostic::new("A number is missing.", span.offset));
    }
    if !is_decimal_literal(span.text) {
        return Err(Diagnostic::new(
            format!("\"{}\" is not a number.", span.text),
            span.offset,
        ));
    }
    match span.text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(Diagnostic::new(
            format!("\"{}\" is out of range.", span.text),
            span.offset,
        )),
    }
}

/// Parses a length: a number, or a number with a unit suffix — `25mm`,
/// `2.5 m`, `6in` — which is converted into the drawing's unit.
///
/// A bare number means what it always meant: so many drawing units. A
/// suffix needs something to convert *to*, so in a unitless drawing
/// (`drawing_unit` is `None`) it is refused rather than guessed at.
///
/// The suffix is whatever letters end the text. That leaves the syntax
/// open: feet-and-inches input (`5'6"`) uses marks that are not letters and
/// can be added beside this without changing what is accepted today.
pub(crate) fn parse_length(
    span: Span<'_>,
    drawing_unit: Option<LengthUnit>,
) -> Result<f64, Diagnostic> {
    let span = span.trimmed();
    let letters = span
        .text
        .bytes()
        .rev()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    let split = span.text.len() - letters;
    let number = span.slice(0, split);
    // No suffix, or only letters (a word, not a length): a plain number or
    // the plain number's diagnostic.
    if letters == 0 || number.text.trim().is_empty() || !is_decimal_literal(number.text.trim()) {
        return parse_number(span);
    }
    let suffix = span.slice(split, span.text.len());
    let Some(unit) = LengthUnit::from_suffix(suffix.text) else {
        return Err(Diagnostic::specific(
            format!(
                "\"{}\" is not a unit of length. Use mm, cm, m, in, or ft.",
                suffix.text
            ),
            suffix.offset,
        ));
    };
    let value = parse_number(number)?;
    match drawing_unit {
        Some(target) => Ok(unit.convert(value, target)),
        None => Err(Diagnostic::specific(
            format!(
                "This drawing has no unit, so \"{}\" cannot be converted. Set one with UNITS.",
                span.text
            ),
            suffix.offset,
        )),
    }
}

fn is_decimal_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));

    let integer_digits = count_digits(&bytes[at..]);
    at += integer_digits;
    let mut fraction_digits = 0;
    if bytes.get(at) == Some(&b'.') {
        fraction_digits = count_digits(&bytes[at + 1..]);
        at += 1 + fraction_digits;
    }
    if integer_digits + fraction_digits == 0 {
        return false;
    }

    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        at += usize::from(matches!(bytes.get(at), Some(b'+' | b'-')));
        let exponent_digits = count_digits(&bytes[at..]);
        if exponent_digits == 0 {
            return false;
        }
        at += exponent_digits;
    }
    at == bytes.len()
}

fn count_digits(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn parse(text: &str) -> Result<f64, Diagnostic> {
        parse_number(Span::new(text, 0))
    }

    #[test]
    fn accepts_the_usual_forms() {
        let cases = [
            ("0", 0.0),
            ("100", 100.0),
            ("-42", -42.0),
            ("+7", 7.0),
            ("3.25", 3.25),
            ("10.", 10.0),
            (".5", 0.5),
            ("-.5", -0.5),
            ("1e3", 1000.0),
            ("2.5E-2", 0.025),
            ("  12.5  ", 12.5),
        ];
        for (text, expected) in cases {
            assert_eq!(parse(text), Ok(expected), "{text:?}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for text in [
            "", "  ", ".", "-", "+", "abc", "1,5", "1.2.3", "1e", "1e+", "--1", "inf", "NaN",
            "0x10", "1 2", "١٢",
        ] {
            assert!(parse(text).is_err(), "{text:?} should be rejected");
        }
    }

    #[test]
    fn rejects_values_that_overflow() {
        let error = parse("1e999").unwrap_err();
        assert!(error.message.contains("out of range"), "{error}");
    }

    #[test]
    fn diagnostics_point_at_the_offending_text() {
        let error = parse_number(Span::new("  x1", 10)).unwrap_err();
        assert_eq!(error.position, 12);
        assert_eq!(error.message, "\"x1\" is not a number.");
    }

    proptest! {
        #[test]
        fn never_panics(text in "\\PC*") {
            let _ = parse(&text);
        }

        #[test]
        fn displayed_floats_parse_back_exactly(value in -1.0e12..1.0e12_f64) {
            prop_assert_eq!(parse(&value.to_string()), Ok(value));
        }
    }
}
