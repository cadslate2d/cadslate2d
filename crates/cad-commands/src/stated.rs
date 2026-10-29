//! What a request states of its sheet (#358): the paper it names, which way
//! up, and the scales it gives. A model short of room has answered with
//! another sheet or scale than the one asked for — A1 for an A2, 1:300 for
//! a 1:200 inset — and said so only in passing. What is stated is read from
//! the request's words here, held against the answer by the review, and
//! never advised away: what does not fit is said, not changed.
//!
//! Read conservatively: a paper only when the request names exactly one,
//! a scale only as a drafter writes one (`1:50`, `2:1`) and not a slope.

use cad_intent::{Intent, Operation, PaperSize};

/// What a request states of its sheet; empty when it states nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stated {
    pub paper: Option<PaperSize>,
    /// Portrait (`true`) or landscape (`false`), when the request says.
    pub portrait: Option<bool>,
    /// Each scale it gives, paper to real: 0.02 for 1:50.
    pub scales: Vec<f64>,
}

/// The scales a drafter gives, 1:N or N:1: a ratio not among them — a
/// slope of 1:12, a time of 10:30 — is not read as one.
const DRAFTING: [f64; 26] = [
    1.0, 2.0, 2.5, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 75.0, 100.0, 125.0, 150.0, 200.0,
    250.0, 300.0, 400.0, 500.0, 750.0, 1000.0, 1250.0, 2000.0, 2500.0, 5000.0,
];

/// Words that make a ratio a slope, in the languages the corpus asks in.
const SLOPES: [&str; 9] = [
    "slope", "gradient", "ramp", "pitch", "inclin", "declive", "caimento", "rampa", "pendente",
];

impl Stated {
    pub fn is_empty(&self) -> bool {
        self.paper.is_none() && self.portrait.is_none() && self.scales.is_empty()
    }
}

/// What `prompt` states of its sheet.
pub fn stated(prompt: &str) -> Stated {
    let chars: Vec<char> = prompt.chars().collect();
    let word = |c: char| c.is_alphanumeric();
    // Papers: `A0` to `A4` as a word of their own.
    let mut papers: Vec<PaperSize> = Vec::new();
    for i in 0..chars.len().saturating_sub(1) {
        let before = i == 0 || !word(chars[i - 1]);
        let after = chars.get(i + 2).is_none_or(|c| !word(*c));
        if chars[i] == 'A' && before && after {
            let paper = match chars[i + 1] {
                '0' => Some(PaperSize::A0),
                '1' => Some(PaperSize::A1),
                '2' => Some(PaperSize::A2),
                '3' => Some(PaperSize::A3),
                '4' => Some(PaperSize::A4),
                _ => None,
            };
            if let Some(paper) = paper
                && !papers.contains(&paper)
            {
                papers.push(paper);
            }
        }
    }
    let lower = prompt.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let portrait = match (
        says(&["portrait", "retrato"]),
        says(&["landscape", "paisagem"]),
    ) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    };
    // Scales: digits, a colon, digits, neither run touching more digits.
    let mut scales: Vec<f64> = Vec::new();
    let number = |from: usize, step: isize| -> (String, usize) {
        let mut at = from as isize;
        let mut text = String::new();
        while at >= 0
            && (at as usize) < chars.len()
            && (chars[at as usize].is_ascii_digit() || chars[at as usize] == '.')
        {
            if step > 0 {
                text.push(chars[at as usize]);
            } else {
                text.insert(0, chars[at as usize]);
            }
            at += step;
        }
        (text, at.max(0) as usize)
    };
    for (i, c) in chars.iter().enumerate() {
        if *c != ':' || i == 0 {
            continue;
        }
        let left = i - 1 - chars[..i].iter().rev().take_while(|c| **c == ' ').count();
        let right = i + 1 + chars[i + 1..].iter().take_while(|c| **c == ' ').count();
        if right >= chars.len() || !chars[left].is_ascii_digit() || !chars[right].is_ascii_digit() {
            continue;
        }
        let (paper_text, start) = number(left, -1);
        let (real_text, _) = number(right, 1);
        let (Ok(paper), Ok(real)) = (paper_text.parse::<f64>(), real_text.parse::<f64>()) else {
            continue;
        };
        let ratio = paper / real;
        // A drafter's scale has a 1 on one side: 1:50, 5:1.
        let n = if paper_text == "1" {
            real
        } else if real_text == "1" {
            paper
        } else {
            continue;
        };
        if !DRAFTING.contains(&n) {
            continue;
        }
        // A slope said just before it is not a scale.
        let lead: String = chars[start.saturating_sub(24)..start].iter().collect();
        if SLOPES.iter().any(|s| lead.to_lowercase().contains(s)) {
            continue;
        }
        if !scales.iter().any(|s| close(*s, ratio)) {
            scales.push(ratio);
        }
    }
    Stated {
        paper: (papers.len() == 1).then(|| papers[0]),
        portrait,
        scales,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a / b - 1.0).abs() < 1e-9
}

/// A ratio as a drafter writes it.
fn written(ratio: f64) -> String {
    crate::compose::ratio_text(ratio)
}

/// Where `intent`, a sheet, departs from what was `stated`: each a sentence
/// a model can act on. Nothing when it draws no sheet.
pub fn departures(intent: &Intent, stated: &Stated) -> Vec<String> {
    departed(intent, stated)
        .into_iter()
        .map(|(finding, _)| finding)
        .collect()
}

/// The same departures as the user is told them, when an answer still
/// makes them: what was asked, and what the sheet is instead.
pub fn departures_said(intent: &Intent, stated: &Stated) -> Vec<String> {
    departed(intent, stated)
        .into_iter()
        .map(|(_, said)| said)
        .collect()
}

/// Each departure: what the model is told, and what the user is.
fn departed(intent: &Intent, stated: &Stated) -> Vec<(String, String)> {
    let blocks: Vec<(PaperSize, bool, f64)> = intent
        .operations
        .iter()
        .filter_map(|op| match op {
            Operation::TitleBlock {
                paper,
                portrait,
                scale,
                ..
            } => Some((*paper, *portrait, scale.unwrap_or(1.0))),
            _ => None,
        })
        .collect();
    if blocks.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let way = |portrait: bool| if portrait { "portrait" } else { "landscape" };
    if let Some(asked) = stated.paper
        && let Some((paper, _, _)) = blocks.iter().find(|(paper, _, _)| *paper != asked)
    {
        out.push((
            format!(
                "The request asks for an {asked:?} sheet, and the title block is {paper:?}. Keep {asked:?}: if what is asked does not fit on it, keep {asked:?} anyway and say in `reply` what does not fit, rather than choosing another sheet."
            ),
            format!("You asked for {asked:?}; this sheet is {paper:?}."),
        ));
    }
    if let Some(asked) = stated.portrait
        && let Some((_, portrait, _)) = blocks.iter().find(|(_, portrait, _)| *portrait != asked)
    {
        out.push((
            format!(
                "The request asks for a {} sheet, and the title block is {}. Keep it {}.",
                way(asked),
                way(*portrait),
                way(asked)
            ),
            format!(
                "You asked for {}; this sheet is {}.",
                way(asked),
                way(*portrait)
            ),
        ));
    }
    // Every scale on the sheet: its own, and each view's.
    let mut drawn: Vec<f64> = blocks.iter().map(|(_, _, n)| 1.0 / n).collect();
    drawn.extend(intent.operations.iter().filter_map(|op| match op {
        Operation::View { ratio, .. } => *ratio,
        _ => None,
    }));
    let missing: Vec<f64> = stated
        .scales
        .iter()
        .copied()
        .filter(|s| !drawn.iter().any(|d| close(*d, *s)))
        .collect();
    if !missing.is_empty() {
        let said: Vec<String> = missing.iter().map(|s| written(*s)).collect();
        let found: Vec<String> = drawn.iter().map(|s| written(*s)).collect();
        out.push((
            format!(
                "The request gives the scale {}, and nothing on the sheet is drawn at it: the sheet and its views are at {}. Keep each scale the request gives — the sheet's in the title block's `scale`, a view's in its own `scale` — and if a view does not fit at it, say so in `reply` rather than choosing another.",
                said.join(" and "),
                found.join(", ")
            ),
            format!(
                "You asked for {}; nothing on this sheet is drawn at it ({}).",
                said.join(" and "),
                found.join(", ")
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paper_and_scales_a_request_names_are_read_and_slopes_are_not() {
        let house = stated(
            "A2 portrait sheet; main floor plan at 1:50. Show site and setback lines in a separate 1:200 inset. A ramp at 1:12, slope 1:20, from 10:30.",
        );
        assert_eq!(house.paper, Some(PaperSize::A2));
        assert_eq!(house.portrait, Some(true));
        assert_eq!(house.scales, vec![1.0 / 50.0, 1.0 / 200.0]);
        // Portuguese, an enlargement, spaces round the colon.
        let detail = stated("Folha A3 paisagem na escala 1 : 100, detalhe em 5:1");
        assert_eq!(detail.paper, Some(PaperSize::A3));
        assert_eq!(detail.portrait, Some(false));
        assert_eq!(detail.scales, vec![0.01, 5.0]);
        // Two papers named, or a section A-A, state no paper.
        assert_eq!(stated("on A3 or A2").paper, None);
        assert_eq!(stated("section A-A and B-B, room A12").paper, None);
        assert!(stated("Draw a circle of radius 25").is_empty());
    }

    fn sheet(paper: &str, extra: &str) -> Intent {
        cad_intent::parse(&format!(
            r#"{{"version": {}, "operations": [
               {{"op": "title_block", "paper": "{paper}", "portrait": true, "scale": 50, "title": "X", "at": [0, 0]}}{extra}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap()
    }

    #[test]
    fn a_sheet_that_departs_from_what_was_stated_is_told_and_one_that_keeps_it_is_not() {
        let asked = stated("A2 portrait sheet; main floor plan at 1:50; a 1:200 inset");
        let inset = |scale: &str| {
            format!(
                r#", {{"op": "view", "id": "site", "placed": "free", "scale": "{scale}", "operations": [
                       {{"op": "rectangle", "corner": [0, 0], "opposite": [10000, 30000]}}]}}"#
            )
        };
        assert_eq!(
            departures(&sheet("A2", &inset("1:200")), &asked),
            Vec::<String>::new()
        );
        let moved = departures(&sheet("A1", &inset("1:300")), &asked);
        assert_eq!(moved.len(), 2, "{moved:?}");
        assert!(
            moved[0].starts_with("The request asks for an A2 sheet, and the title block is A1.")
        );
        assert!(
            moved[1].starts_with(
                "The request gives the scale 1:200, and nothing on the sheet is drawn at it: the sheet and its views are at 1:50, 1:300."
            ),
            "{}",
            moved[1]
        );
        // A document that draws no sheet departs from nothing.
        let circle = cad_intent::parse(&format!(
            r#"{{"version": {}, "operations": [{{"op": "circle", "center": [0, 0], "radius": 5}}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap();
        assert!(departures(&circle, &asked).is_empty());
        // And as the user is told it.
        assert_eq!(
            departures_said(&sheet("A1", &inset("1:300")), &asked),
            vec![
                "You asked for A2; this sheet is A1.".to_owned(),
                "You asked for 1:200; nothing on this sheet is drawn at it (1:50, 1:300)."
                    .to_owned()
            ]
        );
    }
}
