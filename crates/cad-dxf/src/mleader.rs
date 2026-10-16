//! Reading a MULTILEADER and a MLEADERSTYLE (#240, #242; ADR-0033).
//!
//! A MULTILEADER's geometry is in its context data, between `300
//! CONTEXT_DATA{` and `301 }`. Inside it come:
//! - the content: an MTEXT's words (`290` 1, then `304`), or a block
//!   (`296` 1);
//! - then each leader root, between `302 LEADER{` and `303 }`: the last
//!   leader point (`10`, `20`) — our landing — the dogleg's direction and
//!   length, and its leader lines, each between `304 LEADER_LINE{` and
//!   `305 }`, their vertices (`10`, `20`) from the arrowhead's tip.
//!
//! After the context data, `340` names the style by handle and `170` the
//! leaders' kind. What the drawing does not hold is reported, not guessed
//! at.

use cad_core::Point2;
use cad_document::{MLeaderContent, MLeaderEntity, MLeaderStyle, MLeaderStyleId};

use crate::text_codes::plain_mtext;

/// A second leader root, its leaders brought to the first's landing.
pub(crate) const SECOND_ROOT: &str = "(MULTILEADER with two roots)";
/// Block content, not yet held: the note left out.
pub(crate) const BLOCK_CONTENT: &str = "(MULTILEADER block content)";
/// Spline leaders, read straight through their points.
pub(crate) const SPLINE_LEADER: &str = "(MULTILEADER spline leader)";

/// What a MULTILEADER record holds that the drawing keeps.
pub(crate) struct Read {
    pub mleader: MLeaderEntity,
    /// The style's handle, to be found among the MLEADERSTYLE objects.
    pub style: Option<String>,
    /// Report keys for what was left out.
    pub dropped: Vec<&'static str>,
}

/// The multileader the groups describe, or `None` when they make none: no
/// leader line, or a point that is not a number.
pub(crate) fn read(raw: &[(i32, String)]) -> Option<Read> {
    let number = |value: &str| value.trim().parse::<f64>().ok();
    let mut dropped = Vec::new();
    let mut leaders: Vec<Vec<Point2>> = Vec::new();
    let mut landing: Option<Point2> = None;
    let mut text: Option<String> = None;
    let mut has_text = false;
    let mut style = None;
    let mut roots = 0;
    // Where we are: in the context data, a root, a line.
    let (mut context, mut root, mut line) = (false, false, false);
    let mut x: Option<f64> = None;
    let mut current: Vec<Point2> = Vec::new();
    for (code, value) in raw {
        match (*code, value.trim()) {
            (300, "CONTEXT_DATA{") => context = true,
            (301, "}") if context => context = false,
            (302, "LEADER{") if context => {
                root = true;
                roots += 1;
                if roots == 2 {
                    dropped.push(SECOND_ROOT);
                }
            }
            (303, "}") if root => root = false,
            (304, "LEADER_LINE{") if root => {
                line = true;
                current.clear();
            }
            (305, "}") if line => {
                line = false;
                if !current.is_empty() {
                    leaders.push(std::mem::take(&mut current));
                }
            }
            (10, value) if line || (root && roots == 1 && landing.is_none()) => x = number(value),
            (20, value) => {
                if let (Some(px), Some(py)) = (x.take(), number(value)) {
                    let point = Point2::new(px, py);
                    if line {
                        current.push(point);
                    } else if root && roots == 1 && landing.is_none() {
                        landing = Some(point);
                    }
                }
            }
            (290, flag) if context && !root && !has_text && text.is_none() => {
                has_text = flag == "1";
            }
            (304, _) if context && !root && has_text && text.is_none() => {
                text = Some(plain_mtext(value).0);
            }
            (296, "1") if context && !root => dropped.push(BLOCK_CONTENT),
            (340, handle) if !context && style.is_none() => style = Some(handle.to_uppercase()),
            // The leader type after the context data: 2 is a spline.
            (170, "2") if !context && roots > 0 => dropped.push(SPLINE_LEADER),
            _ => {}
        }
    }
    let landing = landing?;
    if leaders.is_empty() || !landing.is_finite() {
        return None;
    }
    let content = match text {
        Some(words) if !words.trim().is_empty() => MLeaderContent::Text(words),
        _ => MLeaderContent::None,
    };
    let mleader = MLeaderEntity {
        leaders,
        landing,
        content,
        style: MLeaderStyleId::FIRST,
    };
    mleader.check().ok()?;
    Some(Read {
        mleader,
        style,
        dropped,
    })
}

/// A MLEADERSTYLE object's values, under `name`: the arrowhead size (44),
/// the landing gap (42), the dogleg (43, none unless 291 is 1), and the
/// text height (45). `None` when the values are none a multileader can be
/// drawn by.
pub(crate) fn read_style(raw: &[(i32, String)], name: &str) -> Option<MLeaderStyle> {
    let value = |code: i32| {
        raw.iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, value)| value.trim().parse::<f64>().ok())
    };
    let defaults = MLeaderStyle::named(name);
    let dogleg = raw
        .iter()
        .find(|(code, _)| *code == 291)
        .is_none_or(|(_, on)| on.trim() != "0");
    let style = MLeaderStyle {
        arrow_size: value(44).unwrap_or(defaults.arrow_size),
        landing_gap: value(42).unwrap_or(defaults.landing_gap),
        dogleg_length: if dogleg {
            value(43).unwrap_or(defaults.dogleg_length)
        } else {
            0.0
        },
        text_height: value(45).unwrap_or(defaults.text_height),
        ..defaults
    };
    style.check().ok().map(|()| style)
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::{
        Drawing, DrawingParts, Entity, MLeaderContent, MLeaderEntity, MLeaderStyle, MLeaderStyleId,
        Properties,
    };
    use proptest::prelude::*;

    use crate::{export, import};

    fn point() -> impl Strategy<Value = Point2> {
        (-1e6..1e6f64, -1e6..1e6f64).prop_map(|(x, y)| Point2::new(x, y))
    }

    proptest! {
        /// Any multileader in any style comes back from a file as it was,
        /// float for float, and the file written again is the same.
        #[test]
        fn multileaders_and_their_styles_survive_a_round_trip(
            leaders in prop::collection::vec(prop::collection::vec(point(), 1..4), 1..4),
            landing in point(),
            words in prop_oneof![Just(None), "[A-Za-z0-9 ]{0,12}".prop_map(Some)],
            arrow in 0.1..10.0f64, gap in 0.0..5.0f64, dogleg in prop_oneof![Just(0.0), 0.1..20.0f64],
            height in 0.1..10.0f64, own_style in any::<bool>(),
        ) {
            let content = match words {
                Some(words) if !words.trim().is_empty() => MLeaderContent::Text(words),
                _ => MLeaderContent::None,
            };
            let style = MLeaderStyle {
                arrow_size: arrow, landing_gap: gap, dogleg_length: dogleg, text_height: height,
                ..MLeaderStyle::named(if own_style { "Callout" } else { MLeaderStyle::STANDARD })
            };
            let id = if own_style { MLeaderStyleId::from_raw(1) } else { MLeaderStyleId::FIRST };
            let mleader = MLeaderEntity { leaders, landing, content, style: id };
            let drawing = Drawing::assemble(DrawingParts {
                mleader_styles: vec![style.clone()],
                entities: vec![(Entity::MLeader(mleader.clone()), Properties::default(), cad_document::LayoutId::MODEL)],
                ..DrawingParts::default()
            }).unwrap();
            let text = export(&drawing);
            let back = import(text.as_bytes()).unwrap();
            prop_assert!(!back.report.has_unsupported_content(), "{:?}", back.report.summary());
            let (_, read) = back.drawing.entities().next().unwrap();
            prop_assert_eq!(read, &Entity::MLeader(mleader));
            prop_assert_eq!(back.drawing.mleader_styles().get(id), Some(&style));
            prop_assert_eq!(export(&back.drawing), text);
        }
    }
}
