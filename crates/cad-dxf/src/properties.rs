//! How DXF spells an entity's and a layer's properties (DESIGN §24, §32):
//! colour numbers (group 62), linetype names (group 6), lineweights (group
//! 370), and a linetype's pattern (groups 49). One place for each
//! convention, shared by the reader and the writer so that what one writes
//! the other reads.

use cad_document::{Color, LinetypeDef, Lineweight};

/// Group 62 of an entity whose colour is its layer's.
pub(crate) const COLOR_BY_LAYER: i64 = 256;
/// Group 62 of an entity whose colour is its block's.
pub(crate) const COLOR_BY_BLOCK: i64 = 0;
/// Group 370 of an entity whose lineweight is its layer's.
pub(crate) const LINEWEIGHT_BY_LAYER: i64 = -1;
pub(crate) const LINEWEIGHT_BY_BLOCK: i64 = -2;
pub(crate) const LINEWEIGHT_DEFAULT: i64 = -3;
/// Group 6 of an entity whose linetype is its layer's, and its block's.
pub(crate) const LINETYPE_BY_LAYER: &str = "BYLAYER";
pub(crate) const LINETYPE_BY_BLOCK: &str = "BYBLOCK";
/// Layer flag bit (group 70): frozen — shown as off here, the nearest thing.
pub(crate) const LAYER_FROZEN: i64 = 1;
/// Layer flag bit: locked.
pub(crate) const LAYER_LOCKED: i64 = 4;
/// Linetype alignment flag (group 72): always `A`, the only kind there is.
pub(crate) const LINETYPE_ALIGNMENT_A: i64 = 65;

/// The colour group 62 names: by layer, by block, or an index. A negative
/// index is an entity switched off in an old file; the colour is kept.
/// `None` for a number that is no colour.
pub(crate) fn color_from_dxf(code: i64) -> Option<Color> {
    match code {
        COLOR_BY_LAYER => Some(Color::ByLayer),
        COLOR_BY_BLOCK => Some(Color::ByBlock),
        1..=255 => Some(Color::Index(code as u8)),
        -255..=-1 => Some(Color::Index(code.unsigned_abs() as u8)),
        _ => None,
    }
}

/// Group 62 for a colour.
pub(crate) fn color_to_dxf(color: Color) -> i64 {
    match color {
        Color::ByLayer => COLOR_BY_LAYER,
        Color::ByBlock => COLOR_BY_BLOCK,
        Color::Index(index) => i64::from(index),
    }
}

/// The lineweight group 370 names. `None` for a width that is not a
/// standard one.
pub(crate) fn lineweight_from_dxf(code: i64) -> Option<Lineweight> {
    match code {
        LINEWEIGHT_BY_LAYER => Some(Lineweight::ByLayer),
        LINEWEIGHT_BY_BLOCK => Some(Lineweight::ByBlock),
        LINEWEIGHT_DEFAULT => Some(Lineweight::Default),
        0..=i64::MAX => u16::try_from(code).ok().and_then(Lineweight::standard),
        _ => None,
    }
}

/// A linetype's pattern as DXF lists it in groups 49: a dash is positive, a
/// gap negative, a dot zero. The drawing keeps dash and gap lengths
/// alternately from a dash, so the list is normalised: neighbouring dashes
/// merge, as do neighbouring gaps; a leading gap moves to the end, where
/// it meets the next repeat the same way; a trailing dash joins the first.
/// `None` for a pattern the drawing cannot hold.
pub(crate) fn pattern_from_dxf(elements: &[f64]) -> Option<Vec<f64>> {
    if elements.iter().any(|length| !length.is_finite()) {
        return None;
    }
    // Runs of (is_dash, length), neighbours of a kind merged.
    let mut runs: Vec<(bool, f64)> = Vec::new();
    for &element in elements {
        let run = (element >= 0.0, element.abs());
        match runs.last_mut() {
            Some((kind, length)) if *kind == run.0 => *length += run.1,
            _ => runs.push(run),
        }
    }
    if runs.len() < 2 {
        // Nothing, only dashes, or only gaps: no pattern to speak of.
        return Some(Vec::new());
    }
    if !runs[0].0 {
        let gap = runs.remove(0);
        match runs.last_mut() {
            Some((false, length)) => *length += gap.1,
            _ => runs.push(gap),
        }
    }
    if runs.len() > 1 && runs.last().is_some_and(|(is_dash, _)| *is_dash) {
        let (_, dash) = runs.pop().expect("checked");
        runs[0].1 += dash;
    }
    let pattern: Vec<f64> = runs.into_iter().map(|(_, length)| length).collect();
    (pattern.len() <= LinetypeDef::MAX_LENGTHS).then_some(pattern)
}

/// A pattern as groups 49 list it: gaps negative.
pub(crate) fn pattern_to_dxf(pattern: &[f64]) -> Vec<f64> {
    pattern
        .iter()
        .enumerate()
        .map(|(index, length)| if index % 2 == 0 { *length } else { -length })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_numbers_read_and_write_both_ways() {
        assert_eq!(color_from_dxf(256), Some(Color::ByLayer));
        assert_eq!(color_from_dxf(0), Some(Color::ByBlock));
        assert_eq!(color_from_dxf(1), Some(Color::Index(1)));
        assert_eq!(color_from_dxf(255), Some(Color::Index(255)));
        assert_eq!(
            color_from_dxf(-7),
            Some(Color::Index(7)),
            "off, but colour 7"
        );
        assert_eq!(color_from_dxf(257), None);
        assert_eq!(color_from_dxf(-300), None);
        for color in [Color::ByLayer, Color::ByBlock, Color::Index(42)] {
            assert_eq!(color_from_dxf(color_to_dxf(color)), Some(color));
        }
    }

    #[test]
    fn lineweights_are_the_standard_widths_or_one_of_three_words() {
        assert_eq!(lineweight_from_dxf(-1), Some(Lineweight::ByLayer));
        assert_eq!(lineweight_from_dxf(-2), Some(Lineweight::ByBlock));
        assert_eq!(lineweight_from_dxf(-3), Some(Lineweight::Default));
        assert_eq!(lineweight_from_dxf(50), Some(Lineweight::Hundredths(50)));
        assert_eq!(lineweight_from_dxf(0), Some(Lineweight::Hundredths(0)));
        assert_eq!(lineweight_from_dxf(51), None);
        assert_eq!(lineweight_from_dxf(-4), None);
        assert_eq!(lineweight_from_dxf(i64::MAX), None);
    }

    #[test]
    fn patterns_are_normalised_to_dash_gap_pairs_from_a_dash() {
        assert_eq!(pattern_from_dxf(&[0.5, -0.25]), Some(vec![0.5, 0.25]));
        assert_eq!(
            pattern_from_dxf(&[0.0, -0.25]),
            Some(vec![0.0, 0.25]),
            "a dot"
        );
        assert_eq!(
            pattern_from_dxf(&[1.25, -0.25, 0.25, -0.25]),
            Some(vec![1.25, 0.25, 0.25, 0.25])
        );
        // Neighbours of a kind merge; a leading gap moves to the end; a
        // trailing dash joins the first.
        assert_eq!(
            pattern_from_dxf(&[0.5, 0.5, -0.25, -0.25]),
            Some(vec![1.0, 0.5])
        );
        assert_eq!(pattern_from_dxf(&[-0.25, 0.5]), Some(vec![0.5, 0.25]));
        assert_eq!(pattern_from_dxf(&[0.5, -0.25, 0.5]), Some(vec![1.0, 0.25]));
        assert_eq!(
            pattern_from_dxf(&[-0.1, 0.5, -0.2, 0.3]),
            Some(vec![0.5, 0.2, 0.3, 0.1])
        );
        // Nothing to draw a pattern with.
        assert_eq!(pattern_from_dxf(&[]), Some(vec![]));
        assert_eq!(pattern_from_dxf(&[0.5]), Some(vec![]));
        assert_eq!(pattern_from_dxf(&[-0.5, -0.5]), Some(vec![]));
        // What the drawing cannot hold.
        assert_eq!(pattern_from_dxf(&[f64::NAN, -1.0]), None);
        assert_eq!(
            pattern_from_dxf(&[1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]),
            None
        );
        // The writer's form reads back.
        let center = [1.25, 0.25, 0.25, 0.25];
        assert_eq!(pattern_to_dxf(&center), vec![1.25, -0.25, 0.25, -0.25]);
        assert_eq!(
            pattern_from_dxf(&pattern_to_dxf(&center)),
            Some(center.to_vec())
        );
    }
}
