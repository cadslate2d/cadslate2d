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
//! `fonts/LICENSE.md`. Its glyphs cover printable ASCII; the accented
//! letters of the Latin languages — `á`, `ç`, `ã`, `ü`, … — are composed
//! from them, the letter and its mark. A character it does not have is
//! drawn as a box the size of a digit, so a string is never silently
//! shorter than it is.
//!
//! # What a layout engine may rely on (DESIGN §34)
//!
//! A dimension places its text, the gap around it, its arrowheads, and its
//! extension lines from these, and must get the same answer the renderer
//! draws. They are held under property tests, here and where they are used:
//!
//! - [`Font::metrics`] gives a string's **width**, **ascent**, and
//!   **descent** at a height. The width is [`Font::advance`] times the
//!   height: additive over concatenation, linear in the height, the same
//!   at every angle. The ascent and descent are the font's, times the
//!   height: a capital is exactly one height tall on the baseline; brackets
//!   rise to the ascent, descenders fall to the descent, and nothing further.
//! - [`Font::strokes`] lays the string along a baseline from an origin at an
//!   angle, and every point of every stroke lies in the box the metrics
//!   describe — `[0, width]` along the baseline, `[-descent, ascent]` up
//!   from it — rotated with it. A character the font lacks keeps that too.
//! - [`Font::wrap`] breaks a paragraph into lines no wider than a width, by
//!   the same advance.

use std::sync::OnceLock;

use cad_core::{Point2, Vec2};

/// The glyph data, in James Hurt's format. Parsed once, on first use.
const SIMPLEX: &str = include_str!("../fonts/futural.jhf");
/// Hershey's Simplex caps run from row −12 to row 9: 21 units tall, with
/// the baseline at row 9. A text height of 1 is one cap height.
const CAP_HEIGHT: f64 = 21.0;
const BASELINE: f64 = 9.0;
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
    /// Glyphs beyond the file's run of characters, built from its own:
    /// the degree sign and the diameter sign a dimension needs, and the
    /// superscript two and three of an area or a volume.
    extras: Vec<(char, Glyph)>,
    /// How far above the baseline any glyph reaches, in cap heights:
    /// brackets and parentheses go above the capitals.
    ascent: f64,
    /// How far below the baseline any glyph reaches, in cap heights.
    descent: f64,
}

/// A string's measurements at a height, in drawing units: the box its
/// strokes lie in, as a layout engine needs them (DESIGN §34).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// From the baseline's start to where the pen ends up.
    pub width: f64,
    /// How far above the baseline the box reaches: the height, and a
    /// little more for brackets.
    pub ascent: f64,
    /// How far below the baseline the box reaches.
    pub descent: f64,
}

/// The glyphs the file lacks that a dimension's text needs, drawn from
/// its own: `°` as a small circle at the cap height, `Ø` as the `O` with a
/// stroke from its bottom left to its top right, and `²` and `³` — a floor
/// plan's m² (#281) — as the `2` and the `3` at six tenths of their size,
/// their tops at the cap height. Each lies within the box of the glyph it
/// is made from, so the font's reach is unchanged.
fn synthesized(glyphs: &[Glyph]) -> Vec<(char, Glyph)> {
    let of = |c: char| glyphs.get((c as u32 - FIRST_CHAR) as usize);
    let mut extras = Vec::new();
    if let Some(digit) = of('0') {
        let advance = digit.advance * 0.6;
        let radius = 0.12;
        let center = Vec2::new(advance / 2.0, 1.0 - radius);
        let ring: Vec<Vec2> = (0..=12)
            .map(|step| {
                let angle = f64::from(step) / 12.0 * std::f64::consts::TAU;
                center + Vec2::new(radius * angle.cos(), radius * angle.sin())
            })
            .collect();
        extras.push((
            '°',
            Glyph {
                strokes: vec![ring],
                advance,
            },
        ));
    }
    if let Some(o) = of('O') {
        let points = || o.strokes.iter().flatten();
        let (mut low, mut high) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
        for point in points() {
            low = Vec2::new(low.x.min(point.x), low.y.min(point.y));
            high = Vec2::new(high.x.max(point.x), high.y.max(point.y));
        }
        if low.x < high.x {
            let mut strokes = o.strokes.clone();
            strokes.push(vec![low, high]);
            extras.push((
                'Ø',
                Glyph {
                    strokes,
                    advance: o.advance,
                },
            ));
        }
    }
    for (superscript, digit) in [('²', '2'), ('³', '3')] {
        if let Some(glyph) = of(digit) {
            // Six tenths the size, raised so its top stays where the
            // digit's is: within the digit's own box.
            let raised = 1.0 - SUPERSCRIPT;
            let strokes = glyph
                .strokes
                .iter()
                .map(|stroke| {
                    stroke
                        .iter()
                        .map(|p| Vec2::new(p.x * SUPERSCRIPT, raised + p.y * SUPERSCRIPT))
                        .collect()
                })
                .collect();
            extras.push((
                superscript,
                Glyph {
                    strokes,
                    advance: glyph.advance * SUPERSCRIPT,
                },
            ));
        }
    }
    extras
}

/// A diacritical mark, drawn in a unit box — x across, y up — that is
/// laid over the letter's ink (or, for the cedilla, under it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Acute,
    Grave,
    Circumflex,
    Tilde,
    Diaeresis,
    Ring,
    Cedilla,
}

impl Mark {
    fn strokes(self) -> Vec<Vec<Vec2>> {
        let line = |points: &[(f64, f64)]| points.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
        let dot = |x: f64| {
            line(&[
                (x - 0.08, 0.4),
                (x + 0.08, 0.4),
                (x + 0.08, 0.6),
                (x - 0.08, 0.6),
                (x - 0.08, 0.4),
            ])
        };
        match self {
            Self::Acute => vec![line(&[(0.3, 0.0), (0.7, 1.0)])],
            Self::Grave => vec![line(&[(0.3, 1.0), (0.7, 0.0)])],
            Self::Circumflex => vec![line(&[(0.1, 0.0), (0.5, 1.0), (0.9, 0.0)])],
            Self::Tilde => vec![line(&[
                (0.0, 0.2),
                (0.25, 0.8),
                (0.5, 0.5),
                (0.75, 0.2),
                (1.0, 0.8),
            ])],
            Self::Diaeresis => vec![dot(0.2), dot(0.8)],
            Self::Ring => vec![
                (0..=12)
                    .map(|step| {
                        let angle = f64::from(step) / 12.0 * std::f64::consts::TAU;
                        Vec2::new(0.5 + 0.3 * angle.cos(), 0.5 + 0.5 * angle.sin())
                    })
                    .collect(),
            ],
            // Under the letter, from the baseline down: y from 0 to −1.
            Self::Cedilla => vec![line(&[
                (0.5, 0.0),
                (0.5, -0.3),
                (0.85, -0.5),
                (0.5, -1.0),
                (0.15, -0.9),
            ])],
        }
    }
}

/// The accented letters, as the letter and its mark.
const ACCENTED: &[(char, char, Mark)] = &[
    ('á', 'a', Mark::Acute),
    ('à', 'a', Mark::Grave),
    ('â', 'a', Mark::Circumflex),
    ('ã', 'a', Mark::Tilde),
    ('ä', 'a', Mark::Diaeresis),
    ('å', 'a', Mark::Ring),
    ('é', 'e', Mark::Acute),
    ('è', 'e', Mark::Grave),
    ('ê', 'e', Mark::Circumflex),
    ('ë', 'e', Mark::Diaeresis),
    ('í', 'i', Mark::Acute),
    ('ì', 'i', Mark::Grave),
    ('î', 'i', Mark::Circumflex),
    ('ï', 'i', Mark::Diaeresis),
    ('ó', 'o', Mark::Acute),
    ('ò', 'o', Mark::Grave),
    ('ô', 'o', Mark::Circumflex),
    ('õ', 'o', Mark::Tilde),
    ('ö', 'o', Mark::Diaeresis),
    ('ú', 'u', Mark::Acute),
    ('ù', 'u', Mark::Grave),
    ('û', 'u', Mark::Circumflex),
    ('ü', 'u', Mark::Diaeresis),
    ('ý', 'y', Mark::Acute),
    ('ÿ', 'y', Mark::Diaeresis),
    ('ñ', 'n', Mark::Tilde),
    ('ç', 'c', Mark::Cedilla),
    ('Á', 'A', Mark::Acute),
    ('À', 'A', Mark::Grave),
    ('Â', 'A', Mark::Circumflex),
    ('Ã', 'A', Mark::Tilde),
    ('Ä', 'A', Mark::Diaeresis),
    ('Å', 'A', Mark::Ring),
    ('É', 'E', Mark::Acute),
    ('È', 'E', Mark::Grave),
    ('Ê', 'E', Mark::Circumflex),
    ('Ë', 'E', Mark::Diaeresis),
    ('Í', 'I', Mark::Acute),
    ('Ì', 'I', Mark::Grave),
    ('Î', 'I', Mark::Circumflex),
    ('Ï', 'I', Mark::Diaeresis),
    ('Ó', 'O', Mark::Acute),
    ('Ò', 'O', Mark::Grave),
    ('Ô', 'O', Mark::Circumflex),
    ('Õ', 'O', Mark::Tilde),
    ('Ö', 'O', Mark::Diaeresis),
    ('Ú', 'U', Mark::Acute),
    ('Ù', 'U', Mark::Grave),
    ('Û', 'U', Mark::Circumflex),
    ('Ü', 'U', Mark::Diaeresis),
    ('Ý', 'Y', Mark::Acute),
    ('Ñ', 'N', Mark::Tilde),
    ('Ç', 'C', Mark::Cedilla),
];

/// Where a small letter's mark goes, in cap heights: above its x-height
/// and below the capitals' top.
const SMALL_MARK: (f64, f64) = (0.8, 0.98);
/// A mark's width, in cap heights.
const MARK_WIDTH: f64 = 0.32;

/// The accented letters, composed: the letter's strokes — an `i` without
/// its dot — and the mark over its ink, between its x-height and the cap
/// height for a small letter, between the cap height and the font's
/// ascent for a capital; a cedilla hangs from the baseline into the
/// descent. Each keeps its letter's advance, and nothing reaches past the
/// font's ascent or descent, so a layout from the metrics still holds.
fn accented(glyphs: &[Glyph], ascent: f64, descent: f64) -> Vec<(char, Glyph)> {
    let of = |c: char| glyphs.get((c as u32 - FIRST_CHAR) as usize);
    let capital = (1.04, (ascent - 0.02).min(1.2));
    let mut extras = Vec::new();
    for &(accented, letter, mark) in ACCENTED {
        let Some(base) = of(letter) else { continue };
        let mut strokes: Vec<Vec<Vec2>> = base.strokes.clone();
        if letter == 'i' {
            // The dot gives way to the mark.
            strokes.retain(|stroke| stroke.iter().any(|p| p.y < 0.75));
        }
        let xs = || strokes.iter().flatten().map(|p| p.x);
        let middle = (xs().fold(f64::MAX, f64::min) + xs().fold(f64::MIN, f64::max)) / 2.0;
        let place = |p: &Vec2, bottom: f64, top: f64| {
            Vec2::new(
                middle + (p.x - 0.5) * MARK_WIDTH,
                bottom + p.y * (top - bottom),
            )
        };
        let (bottom, top) = if letter.is_uppercase() {
            capital
        } else {
            SMALL_MARK
        };
        for stroke in mark.strokes() {
            strokes.push(
                stroke
                    .iter()
                    .map(|p| match mark {
                        // From the baseline down, two thirds of the descent.
                        Mark::Cedilla => place(p, 0.0, descent * 2.0 / 3.0),
                        _ => place(p, bottom, top),
                    })
                    .collect(),
            );
        }
        extras.push((
            accented,
            Glyph {
                strokes,
                advance: base.advance,
            },
        ));
    }
    extras
}

/// A superscript's size, as a share of the digit it is made from.
const SUPERSCRIPT: f64 = 0.6;

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
        let ys = || {
            glyphs
                .iter()
                .flat_map(|glyph| glyph.strokes.iter().flatten().map(|p| p.y))
        };
        let ascent = ys().fold(1.0_f64, f64::max);
        let descent = -ys().fold(0.0_f64, f64::min);
        let mut extras = synthesized(&glyphs);
        extras.extend(accented(&glyphs, ascent, descent));
        Ok(Font {
            glyphs,
            extras,
            ascent,
            descent,
        })
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
        match (c as u32).checked_sub(FIRST_CHAR) {
            Some(index) if (index as usize) < self.glyphs.len() => self.glyphs.get(index as usize),
            _ => self
                .extras
                .iter()
                .find(|(extra, _)| *extra == c)
                .map(|(_, glyph)| glyph),
        }
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

    /// The box `text` occupies at `height` (the cap height, in drawing
    /// units), in those units: what a dimension lays itself out from.
    pub fn metrics(&self, text: &str, height: f64) -> Metrics {
        Metrics {
            width: self.advance(text) * height,
            ascent: self.ascent * height,
            descent: self.descent * height,
        }
    }

    /// How far above the baseline the font reaches, in cap heights: at
    /// least 1, more where brackets and parentheses rise above the capitals.
    pub fn ascent(&self) -> f64 {
        self.ascent
    }

    /// How far below the baseline the font reaches, in cap heights.
    pub fn descent(&self) -> f64 {
        self.descent
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

impl Font {
    /// Breaks `text` into lines no wider than `width` cap heights, at
    /// spaces: the lines a paragraph is set in. An explicit line break
    /// always breaks; a word wider than the width stands on a line of its
    /// own, whole, rather than being cut; spaces at the end of a line are
    /// dropped, and never count against the width. `width` of zero or
    /// less means no wrapping: only the explicit breaks.
    pub fn wrap(&self, text: &str, width: f64) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            if width <= 0.0 {
                lines.push(paragraph.trim_end().to_owned());
                continue;
            }
            // Words and the runs of spaces between them, in turn.
            let mut line = String::new();
            let mut pending = String::new();
            for token in runs(paragraph) {
                if token.starts_with(' ') {
                    pending.push_str(token);
                    continue;
                }
                let candidate = format!("{line}{pending}{token}");
                if line.trim().is_empty() || self.advance(&candidate) <= width {
                    line = candidate;
                } else {
                    lines.push(line.trim_end().to_owned());
                    line = token.to_owned();
                }
                pending.clear();
            }
            lines.push(line.trim_end().to_owned());
        }
        lines
    }
}

/// The maximal runs of spaces and of other characters in `text`, in order.
fn runs(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let spaces = first == ' ';
        let end = rest
            .find(|c: char| (c == ' ') != spaces)
            .unwrap_or(rest.len());
        let (run, after) = rest.split_at(end);
        rest = after;
        Some(run)
    })
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
        assert!(font.glyph('€').is_none(), "neither ASCII nor composed");
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
        assert!(close(font.descent(), 7.0 / 21.0), "{}", font.descent());
        // Brackets rise above the capitals; nothing rises above the ascent.
        assert!(
            font.ascent() > 1.0 && close(font.ascent(), 25.0 / 21.0),
            "{}",
            font.ascent()
        );
        let highest = font
            .glyph('[')
            .unwrap()
            .strokes
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(0.0, f64::max);
        assert!(close(highest, font.ascent()));
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
    fn superscripts_are_small_raised_digits_within_the_cap_height() {
        let font = Font::simplex();
        for (superscript, digit) in [('²', '2'), ('³', '3')] {
            let small = font.glyph(superscript).expect("a superscript");
            let full = font.glyph(digit).unwrap();
            assert!(close(small.advance, full.advance * 0.6));
            for point in small.strokes.iter().flatten() {
                assert!(point.y >= 0.4 - 1e-9 && point.y <= 1.0 + 1e-9, "{point:?}");
            }
        }
        let area = font.metrics("12.50 m²", 1.0);
        assert!(close(area.ascent, font.ascent()));
    }

    #[test]
    fn accented_letters_are_the_letter_and_its_mark_within_the_font() {
        let font = Font::simplex();
        for &(accented, letter, _) in ACCENTED {
            let glyph = font.glyph(accented).expect("composed");
            let base = font.glyph(letter).unwrap();
            assert!(
                close(glyph.advance, base.advance),
                "{accented} keeps {letter}'s advance"
            );
            assert!(glyph.strokes.len() > base.strokes.len() - usize::from(letter == 'i'));
            for point in glyph.strokes.iter().flatten() {
                assert!(
                    point.y <= font.ascent() + 1e-9 && point.y >= -font.descent() - 1e-9,
                    "{accented}: {point:?}"
                );
            }
        }
        // A small letter's mark is above its x-height, a capital's above
        // the cap height; a cedilla below the baseline.
        let top = |c: char| {
            font.glyph(c)
                .unwrap()
                .strokes
                .iter()
                .flatten()
                .map(|p| p.y)
                .fold(f64::MIN, f64::max)
        };
        let bottom = |c: char| {
            font.glyph(c)
                .unwrap()
                .strokes
                .iter()
                .flatten()
                .map(|p| p.y)
                .fold(f64::MAX, f64::min)
        };
        assert!(top('ó') > top('o') + 0.1 && top('ó') <= 1.0);
        assert!(top('Ó') > 1.0);
        assert!(bottom('ç') < -0.1);
        // An í has no dot under its accent.
        assert!(font.glyph('í').unwrap().strokes.len() == font.glyph('i').unwrap().strokes.len());
        // Portuguese room names are drawn with no box.
        for word in [
            "Escritório",
            "Suíte",
            "Área de serviço",
            "Cozinha e lavanderia",
            "Ático",
        ] {
            for c in word.chars() {
                assert!(font.glyph(c).is_some(), "{c} in {word}");
            }
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

    #[test]
    fn a_paragraph_wraps_at_spaces_to_the_width_and_at_explicit_breaks() {
        let font = Font::simplex();
        let width = font.advance("one two") + 0.01;
        assert_eq!(
            font.wrap("one two three four", width),
            vec!["one two", "three", "four"]
        );
        let wider = font.advance("two three") + 0.01;
        assert_eq!(font.wrap("one\ntwo three", wider), vec!["one", "two three"]);
        assert_eq!(font.wrap("", width), vec![""]);
        assert_eq!(
            font.wrap("a\n\nb", width),
            vec!["a", "", "b"],
            "a blank line stays"
        );
        // A word wider than the width stands alone, whole.
        let long = "supercalifragilistic";
        assert_eq!(
            font.wrap(&format!("a {long} b"), width),
            vec!["a", long, "b"]
        );
        // Spaces at the end of a line are dropped and do not count; spaces
        // inside a line are kept.
        assert_eq!(font.wrap("one two    ", width), vec!["one two"]);
        assert_eq!(font.wrap("one  two", wider), vec!["one  two"]);
        assert_eq!(
            font.wrap("  one", wider),
            vec!["  one"],
            "an indent is kept"
        );
        assert_eq!(
            font.wrap("one two three   ", width),
            vec!["one two", "three"]
        );
        // No width: only the explicit breaks.
        assert_eq!(
            font.wrap("one two three\nfour", 0.0),
            vec!["one two three", "four"]
        );
        // Every line fits, unless it is one word.
        for line in font.wrap("the quick brown fox jumps over the lazy dog", 3.0) {
            assert!(font.advance(&line) <= 3.0 || !line.contains(' '), "{line}");
        }
    }

    proptest! {
        /// Wrapping loses no word and keeps their order, and every line
        /// with a space in it fits.
        #[test]
        fn wrapping_keeps_every_word_in_order(
            text in "[ -~\n]{0,60}",
            width in 0.5..20.0f64,
        ) {
            let font = Font::simplex();
            let lines = font.wrap(&text, width);
            let words: Vec<&str> = text.split(['\n', ' ']).filter(|w| !w.is_empty()).collect();
            let wrapped: Vec<&str> = lines.iter().flat_map(|l| l.split(' ')).filter(|w| !w.is_empty()).collect();
            prop_assert_eq!(wrapped, words);
            for line in &lines {
                prop_assert!(font.advance(line) <= width + 1e-9 || !line.trim().contains(' '), "{}", line);
                prop_assert!(!line.ends_with(' '));
            }
            prop_assert!(lines.len() > text.matches('\n').count(), "one line per break at least");
        }

        /// The metrics are linear in the height and the same at every
        /// angle; a capital is one height tall; nothing reaches beyond
        /// the ascent or the descent.
        #[test]
        fn metrics_are_linear_in_the_height_and_bound_every_glyph(
            text in "[ -~]{0,12}",
            height in 0.01..1000.0f64,
            factor in 0.1..10.0f64,
        ) {
            let font = Font::simplex();
            let one = font.metrics(&text, height);
            let scaled = font.metrics(&text, height * factor);
            prop_assert!(close(scaled.width, one.width * factor));
            prop_assert!(close(scaled.ascent, one.ascent * factor));
            prop_assert!(close(scaled.descent, one.descent * factor));
            prop_assert!(one.ascent >= height - 1e-9 && one.descent >= 0.0);
            prop_assert!(close(one.width, font.advance(&text) * height));
            // The metrics are the height times the font's: the font's are the
            // greatest reach of any glyph, so no glyph exceeds them.
            for c in text.chars() {
                if let Some(glyph) = font.glyph(c) {
                    for point in glyph.strokes.iter().flatten() {
                        prop_assert!(point.y * height <= one.ascent + 1e-9);
                        prop_assert!(-point.y * height <= one.descent + 1e-9);
                    }
                }
            }
        }

        /// Every stroke of a string lies in the box its metrics describe,
        /// measured along and up from the baseline, at any angle — the
        /// characters the font lacks included.
        #[test]
        fn strokes_lie_within_the_metrics_box_at_any_angle(
            text in "[ -~é€]{0,12}",
            height in 0.1..100.0f64,
            angle in -7.0..7.0f64,
            ox in -1000.0..1000.0f64,
            oy in -1000.0..1000.0f64,
        ) {
            let font = Font::simplex();
            let origin = Point2::new(ox, oy);
            let metrics = font.metrics(&text, height);
            let (sin, cos) = angle.sin_cos();
            let slack = 1e-9 * (1.0 + height + ox.abs() + oy.abs());
            for point in font.strokes(&text, origin, height, angle).iter().flatten() {
                let local = *point - origin;
                let along = local.x * cos + local.y * sin;
                let up = -local.x * sin + local.y * cos;
                prop_assert!(along >= -slack && along <= metrics.width + slack, "{along} of {}", metrics.width);
                prop_assert!(up >= -metrics.descent - slack && up <= metrics.ascent + slack, "{up}");
            }
        }

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
            let reach = (font.advance(&a) + font.ascent() + font.descent()) * height + 1e-6;
            for point in strokes.iter().flatten() {
                prop_assert!(point.x.is_finite() && point.y.is_finite());
                prop_assert!(point.distance_to(Point2::new(3.0, -4.0)) <= reach);
            }
        }
    }
}
