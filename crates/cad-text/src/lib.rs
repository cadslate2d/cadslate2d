//! Text as geometry (DESIGN §23.5, §34; ADR-0016).
//!
//! A CAD text is drawn with a *stroke font*: every glyph is a few
//! polylines, and a string is those polylines laid along a baseline. So a
//! text is lines, drawn by the renderer that draws every other line, sharp
//! at every zoom and any angle; and its metrics — how wide a string is, how
//! tall — are arithmetic over the glyph table, computed here on the host
//! with no browser in sight, which is what bounds, picking, headless tests,
//! and later dimensions need.
//!
//! The font is Hershey's Simplex, the sans-serif single-stroke face that
//! classic drafting programs ship as their default; see
//! `fonts/LICENSE.md`. Its glyphs cover printable ASCII. A character it
//! does not have is drawn as a box the size of a digit, so a string is
//! never silently shorter than it is.

use std::sync::OnceLock;

use cad_core::{Point2, Vec2};

/// The glyph data, in James Hurt's format. Parsed once, on first use.
const SIMPLEX: &str = include_str!("../fonts/futural.jhf");
/// Hershey's Simplex caps run from row −12 to row 9: 21 units tall, with
/// the baseline at row 9. A text height of 1 is one cap height.
const CAP_HEIGHT: f64 = 21.0;
const BASELINE: f64 = 9.0;
/// Descenders reach row 16 in Simplex: 7 units below the baseline.
const DESCENT: f64 = 7.0;
/// The first glyph of the file is the space.
const FIRST_CHAR: u32 = ' ' as u32;

/// One glyph: its strokes, in cap heights, on a baseline at y = 0 with x
/// from the glyph's left bearing, and how far the pen advances past it.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The polylines that draw the glyph, in cap heights, relative to the
    /// pen position at its left.
    pub strokes: Vec<Vec<Vec2>>,
    /// How far the pen moves on, in cap heights.
    pub advance: f64,
}

/// A stroke font: a glyph per character it has.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    glyphs: Vec<Glyph>,
}

/// Why a glyph line could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    /// The line is shorter than its header, or shorter than its count says.
    Truncated { line: usize },
    /// The vertex count is not a number.
    BadCount { line: usize },
}

impl Font {
    /// The built-in font, parsed on first use.
    pub fn simplex() -> &'static Font {
        static FONT: OnceLock<Font> = OnceLock::new();
        FONT.get_or_init(|| Font::parse_jhf(SIMPLEX).expect("the bundled font is well formed"))
    }

    /// Reads a `.jhf` file: glyphs for consecutive characters from the
    /// space onwards.
    pub fn parse_jhf(text: &str) -> Result<Font, FontError> {
        let glyphs = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| parse_glyph(line, index + 1))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Font { glyphs })
    }

    /// How many characters the font has glyphs for.
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// The glyph for `c`, if the font has one. Control characters have
    /// none, whatever the file holds at their place.
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        if c.is_control() {
            return None;
        }
        let index = (c as u32).checked_sub(FIRST_CHAR)?;
        self.glyphs.get(index as usize)
    }

    /// The glyph drawn for a character the font lacks: a box the width of
    /// a digit, so the string keeps its length and the gap is visible.
    fn missing(&self) -> Glyph {
        let advance = self.glyph('0').map_or(0.8, |digit| digit.advance);
        let (left, right) = (advance * 0.15, advance * 0.85);
        let (bottom, top) = (0.0, 1.0);
        Glyph {
            strokes: vec![vec![
                Vec2::new(left, bottom),
                Vec2::new(right, bottom),
                Vec2::new(right, top),
                Vec2::new(left, top),
                Vec2::new(left, bottom),
            ]],
            advance,
        }
    }

    /// How wide `text` is, in cap heights: where the pen ends up.
    pub fn advance(&self, text: &str) -> f64 {
        text.chars()
            .map(|c| {
                self.glyph(c)
                    .map_or_else(|| self.missing().advance, |g| g.advance)
            })
            .sum()
    }

    /// How far below the baseline the font reaches, in cap heights.
    pub fn descent(&self) -> f64 {
        DESCENT / CAP_HEIGHT
    }

    /// The strokes of `text` at `height` (the cap height, in drawing
    /// units), laid along a baseline that starts at `origin` and runs at
    /// `angle` radians from +X.
    pub fn strokes(&self, text: &str, origin: Point2, height: f64, angle: f64) -> Vec<Vec<Point2>> {
        let (sin, cos) = angle.sin_cos();
        let place = |pen: f64, point: Vec2| {
            let x = (pen + point.x) * height;
            let y = point.y * height;
            origin + Vec2::new(x * cos - y * sin, x * sin + y * cos)
        };
        let mut pen = 0.0;
        let mut strokes = Vec::new();
        for c in text.chars() {
            let fallback;
            let glyph = match self.glyph(c) {
                Some(glyph) => glyph,
                None => {
                    fallback = self.missing();
                    &fallback
                }
            };
            for stroke in &glyph.strokes {
                strokes.push(stroke.iter().map(|point| place(pen, *point)).collect());
            }
            pen += glyph.advance;
        }
        strokes
    }
}

/// One line of a `.jhf` file.
fn parse_glyph(line: &str, number: usize) -> Result<Glyph, FontError> {
    let bytes = line.as_bytes();
    if bytes.len() < 10 {
        return Err(FontError::Truncated { line: number });
    }
    let count: usize = line[5..8]
        .trim()
        .parse()
        .map_err(|_| FontError::BadCount { line: number })?;
    let coordinate = |byte: u8| f64::from(i32::from(byte) - i32::from(b'R'));
    let (pairs, _) = bytes[8..].as_chunks::<2>();
    let pairs: Vec<(u8, u8)> = pairs.iter().map(|pair| (pair[0], pair[1])).collect();
    if pairs.len() < count {
        return Err(FontError::Truncated { line: number });
    }
    let (left, right) = pairs[0];
    let (left, right) = (coordinate(left), coordinate(right));
    let mut strokes: Vec<Vec<Vec2>> = Vec::new();
    let mut current: Vec<Vec2> = Vec::new();
    for &(x, y) in &pairs[1..count] {
        if x == b' ' && y == b'R' {
            if current.len() > 1 {
                strokes.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            continue;
        }
        // The pen is at the left hand position; the baseline is row 9,
        // and rows grow downward.
        current.push(Vec2::new(
            (coordinate(x) - left) / CAP_HEIGHT,
            (BASELINE - coordinate(y)) / CAP_HEIGHT,
        ));
    }
    if current.len() > 1 {
        strokes.push(current);
    }
    Ok(Glyph {
        strokes,
        advance: (right - left) / CAP_HEIGHT,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_bundled_font_has_printable_ascii() {
        let font = Font::simplex();
        assert_eq!(font.len(), 96);
        for c in ' '..='~' {
            assert!(font.glyph(c).is_some(), "{c:?}");
        }
        assert!(font.glyph('\u{7f}').is_none());
        assert!(font.glyph('é').is_none());
        assert!(font.glyph('\n').is_none());
    }

    #[test]
    fn a_capital_is_one_cap_height_tall_on_the_baseline() {
        let font = Font::simplex();
        let h = font.glyph('H').unwrap();
        let ys: Vec<f64> = h.strokes.iter().flatten().map(|p| p.y).collect();
        assert!(ys.iter().any(|y| close(*y, 0.0)), "stands on the baseline");
        assert!(ys.iter().any(|y| close(*y, 1.0)), "reaches the cap height");
        assert_eq!(h.strokes.len(), 3, "two stems and a bar");
        let xs: Vec<f64> = h.strokes.iter().flatten().map(|p| p.x).collect();
        assert!(xs.iter().all(|x| *x >= 0.0), "drawn right of the pen");
        assert!(
            h.advance > xs.iter().cloned().fold(0.0, f64::max),
            "with a right bearing"
        );
        // A descender goes below the baseline, and never below the descent.
        let g = font.glyph('g').unwrap();
        let lowest = g.strokes.iter().flatten().map(|p| p.y).fold(0.0, f64::min);
        assert!(lowest < 0.0 && lowest >= -font.descent() - 1e-9);
        // The space has no strokes and some width.
        let space = font.glyph(' ').unwrap();
        assert!(space.strokes.is_empty() && space.advance > 0.0);
    }

    #[test]
    fn a_string_is_laid_along_its_baseline_at_its_height_and_angle() {
        let font = Font::simplex();
        let flat = font.strokes("HI", Point2::new(10.0, 20.0), 5.0, 0.0);
        assert_eq!(flat.len(), 3 + font.glyph('I').unwrap().strokes.len());
        let ys: Vec<f64> = flat.iter().flatten().map(|p| p.y).collect();
        assert!(ys.iter().all(|y| *y >= 20.0 - 1e-9 && *y <= 25.0 + 1e-9));
        let width = font.advance("HI") * 5.0;
        let xs: Vec<f64> = flat.iter().flatten().map(|p| p.x).collect();
        assert!(
            xs.iter()
                .all(|x| *x >= 10.0 - 1e-9 && *x <= 10.0 + width + 1e-9)
        );

        // Turned a quarter, x becomes y.
        let up = font.strokes(
            "HI",
            Point2::new(10.0, 20.0),
            5.0,
            std::f64::consts::FRAC_PI_2,
        );
        for (a, b) in flat.iter().flatten().zip(up.iter().flatten()) {
            assert!(close(b.x, 10.0 - (a.y - 20.0)) && close(b.y, 20.0 + (a.x - 10.0)));
        }
    }

    #[test]
    fn a_character_the_font_lacks_is_a_box_and_keeps_the_width() {
        let font = Font::simplex();
        let with = font.advance("a€b");
        let without = font.advance("a0b");
        assert!(
            close(with, without),
            "a missing glyph is as wide as a digit"
        );
        let strokes = font.strokes("€", Point2::ORIGIN, 1.0, 0.0);
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].len(), 5, "a closed box");
        assert!(font.advance("") == 0.0);
    }

    #[test]
    fn a_file_that_is_cut_short_is_refused() {
        assert_eq!(
            Font::parse_jhf("12345  9MWRFRT").err(),
            Some(FontError::Truncated { line: 1 })
        );
        assert_eq!(
            Font::parse_jhf("12345 xxJZ").err(),
            Some(FontError::BadCount { line: 1 })
        );
        assert_eq!(
            Font::parse_jhf("1234").err(),
            Some(FontError::Truncated { line: 1 })
        );
        let one = Font::parse_jhf("12345  1JZ\n\n").unwrap();
        assert_eq!(one.len(), 1);
    }

    proptest! {
        /// Width is additive over concatenation, and the strokes of a
        /// string are finite and within its box.
        #[test]
        fn width_adds_up_and_strokes_stay_in_the_box(
            a in "[ -~]{0,8}",
            b in "[ -~]{0,8}",
            height in 0.1..100.0f64,
            angle in -7.0..7.0f64,
        ) {
            let font = Font::simplex();
            let both = format!("{a}{b}");
            let sum = font.advance(&a) + font.advance(&b);
            prop_assert!(close(font.advance(&both), sum));
            let strokes = font.strokes(&a, Point2::new(3.0, -4.0), height, angle);
            let reach = (font.advance(&a) + 1.0 + font.descent()) * height + 1e-6;
            for point in strokes.iter().flatten() {
                prop_assert!(point.x.is_finite() && point.y.is_finite());
                prop_assert!(point.distance_to(Point2::new(3.0, -4.0)) <= reach);
            }
        }
    }
}
