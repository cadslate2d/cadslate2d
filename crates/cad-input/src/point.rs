//! Coordinate entry (DESIGN §14).
//!
//! ```text
//! 100,200     absolute Cartesian
//! @500,0      relative Cartesian
//! @500<45     relative polar: distance < angle in degrees, counter-clockwise from +X
//! ```

use cad_core::{Point2, Vec2};

use crate::Diagnostic;
use cad_core::LengthUnit;

use crate::number::{Span, parse_length, parse_number};

/// A point as typed, before it is resolved against the previous point.
///
/// Parsing and resolution are separate so the parser stays free of session
/// state: only the command runtime knows what "previous point" means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointInput {
    Absolute(Point2),
    RelativeCartesian(Vec2),
    RelativePolar { distance: f64, angle_degrees: f64 },
}

impl PointInput {
    /// The world point this input denotes, given the previous point.
    pub fn resolve(self, last_point: Point2) -> Point2 {
        match self {
            PointInput::Absolute(point) => point,
            PointInput::RelativeCartesian(offset) => last_point + offset,
            PointInput::RelativePolar {
                distance,
                angle_degrees,
            } => last_point + Vec2::from_polar(distance, angle_degrees.to_radians()),
        }
    }
}

/// Parses one point in any supported syntax, for a unitless drawing: plain
/// numbers only.
pub fn parse_point(text: &str) -> Result<PointInput, Diagnostic> {
    parse_point_in(text, None)
}

/// Parses one point in any supported syntax. Coordinates and distances may
/// carry a unit suffix — `100mm,2m`, `@500mm<45` — and are converted into
/// `drawing_unit`; an angle never has one.
pub fn parse_point_in(
    text: &str,
    drawing_unit: Option<LengthUnit>,
) -> Result<PointInput, Diagnostic> {
    let span = Span::new(text, 0).trimmed();
    if span.text.is_empty() {
        return Err(Diagnostic::new("A point is required.", span.offset));
    }

    let relative = span.text.starts_with('@');
    let body = if relative {
        span.slice(1, span.text.len())
    } else {
        span
    };

    // A bare `@` is the previous point again: no offset at all.
    if relative && body.text.trim().is_empty() {
        return Ok(PointInput::RelativeCartesian(Vec2::new(0.0, 0.0)));
    }

    if let Some(separator) = body.text.find('<') {
        if !relative {
            return Err(Diagnostic::new(
                "Polar input must be relative: @distance<angle.",
                span.offset,
            ));
        }
        let distance = parse_length(body.slice(0, separator), drawing_unit)?;
        let angle_degrees = parse_number(body.slice(separator + 1, body.text.len()))?;
        return Ok(PointInput::RelativePolar {
            distance,
            angle_degrees,
        });
    }

    let (x, y) = parse_pair(body, drawing_unit)?;
    Ok(if relative {
        PointInput::RelativeCartesian(Vec2::new(x, y))
    } else {
        PointInput::Absolute(Point2::new(x, y))
    })
}

/// Parses `x,y`. Exactly two components: drawings are 2D.
fn parse_pair(span: Span<'_>, drawing_unit: Option<LengthUnit>) -> Result<(f64, f64), Diagnostic> {
    let Some(comma) = span.text.find(',') else {
        return Err(Diagnostic::new("A point is required, as x,y.", span.offset));
    };
    let rest = span.slice(comma + 1, span.text.len());
    if let Some(extra) = rest.text.find(',') {
        return Err(Diagnostic::new(
            "Only 2D points are supported: enter x,y.",
            rest.offset + extra,
        ));
    }
    Ok((
        parse_length(span.slice(0, comma), drawing_unit)?,
        parse_length(rest, drawing_unit)?,
    ))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    #[test]
    fn a_bare_at_sign_is_the_previous_point_again() {
        for text in ["@", " @ ", "@  "] {
            assert_eq!(
                parse_point(text),
                Ok(PointInput::RelativeCartesian(Vec2::new(0.0, 0.0))),
                "{text:?}"
            );
        }
        let previous = Point2::new(12.5, -3.0);
        assert_eq!(parse_point("@").unwrap().resolve(previous), previous);
    }

    #[test]
    fn parses_each_syntax() {
        let cases = [
            ("100,200", PointInput::Absolute(Point2::new(100.0, 200.0))),
            ("-1.5,.25", PointInput::Absolute(Point2::new(-1.5, 0.25))),
            ("  0 , 0  ", PointInput::Absolute(Point2::ORIGIN)),
            (
                "@500,0",
                PointInput::RelativeCartesian(Vec2::new(500.0, 0.0)),
            ),
            (
                "@-3,-4",
                PointInput::RelativeCartesian(Vec2::new(-3.0, -4.0)),
            ),
            (
                "@500<45",
                PointInput::RelativePolar {
                    distance: 500.0,
                    angle_degrees: 45.0,
                },
            ),
            (
                "@ 10 < -90",
                PointInput::RelativePolar {
                    distance: 10.0,
                    angle_degrees: -90.0,
                },
            ),
            (
                "@1e2<4.5e1",
                PointInput::RelativePolar {
                    distance: 100.0,
                    angle_degrees: 45.0,
                },
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_point(text), Ok(expected), "{text:?}");
        }
    }

    #[test]
    fn rejects_malformed_points_with_a_position() {
        let cases = [
            ("", "A point is required.", 0),
            ("   ", "A point is required.", 3),
            ("100", "A point is required, as x,y.", 0),
            ("100,", "A number is missing.", 4),
            (",200", "A number is missing.", 0),
            ("1,2,3", "Only 2D points are supported: enter x,y.", 3),
            ("a,5", "\"a\" is not a number.", 0),
            ("5,b", "\"b\" is not a number.", 2),
            (
                "500<45",
                "Polar input must be relative: @distance<angle.",
                0,
            ),
            ("@500<", "A number is missing.", 5),
            ("@<45", "A number is missing.", 1),
            ("@5<4<3", "\"4<3\" is not a number.", 3),
            ("@@1,1", "\"@1\" is not a number.", 1),
            ("C", "A point is required, as x,y.", 0),
        ];
        for (text, message, position) in cases {
            let error = parse_point(text).unwrap_err();
            assert_eq!(
                (error.message.as_str(), error.position),
                (message, position),
                "{text:?}"
            );
        }
    }

    #[test]
    fn absolute_points_ignore_the_previous_point() {
        let input = PointInput::Absolute(Point2::new(1.0, 2.0));
        assert_eq!(
            input.resolve(Point2::new(50.0, 50.0)),
            Point2::new(1.0, 2.0)
        );
    }

    #[test]
    fn relative_cartesian_offsets_from_the_previous_point() {
        let input = PointInput::RelativeCartesian(Vec2::new(500.0, -10.0));
        assert_eq!(
            input.resolve(Point2::new(100.0, 100.0)),
            Point2::new(600.0, 90.0)
        );
    }

    #[test]
    fn relative_polar_measures_degrees_counter_clockwise_from_x() {
        let last = Point2::new(10.0, 10.0);
        let east = PointInput::RelativePolar {
            distance: 5.0,
            angle_degrees: 0.0,
        };
        let north = PointInput::RelativePolar {
            distance: 5.0,
            angle_degrees: 90.0,
        };
        let diagonal = PointInput::RelativePolar {
            distance: 1000.0,
            angle_degrees: 45.0,
        };

        assert!(close(east.resolve(last), Point2::new(15.0, 10.0)));
        assert!(close(north.resolve(last), Point2::new(10.0, 15.0)));
        let half = 1000.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(close(
            diagonal.resolve(Point2::ORIGIN),
            Point2::new(half, half)
        ));
    }

    proptest! {
        #[test]
        fn never_panics(text in "\\PC*") {
            let _ = parse_point(&text);
        }

        #[test]
        fn never_panics_on_point_like_text(text in "[@0-9.,<eE+\\- ]{0,24}") {
            let _ = parse_point(&text);
        }

        #[test]
        fn formatted_absolute_points_round_trip(x in -1.0e9..1.0e9_f64, y in -1.0e9..1.0e9_f64) {
            prop_assert_eq!(parse_point(&format!("{x},{y}")), Ok(PointInput::Absolute(Point2::new(x, y))));
        }

        #[test]
        fn diagnostic_positions_stay_inside_the_input(text in "[@0-9.,<a-z ]{0,16}") {
            if let Err(error) = parse_point(&text) {
                prop_assert!(error.position <= text.len());
            }
        }
    }
}
