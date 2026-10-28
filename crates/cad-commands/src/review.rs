//! A drawing's review (#349): what is plainly wrong with a sheet a request
//! made, said so it can be put right before the proposal is shown. Words
//! that run into each other; anything off the sheet; a drawing that fills a
//! sliver of its sheet, as one shrunk to a scale the sheet was already
//! enlarged by; a sheet whose sizes are typed instead of dimensioned. Each
//! finding is a sentence a model can act on. Nothing here changes the
//! drawing.

use cad_core::{Aabb, EntityId};
use cad_document::{Drawing, Entity};

use crate::titleblock::GROUP;

/// The most findings of one kind that are spelled out; the rest are counted.
const SPELLED_OUT: usize = 6;

/// How much of the smaller of two texts' boxes another must cover for the
/// two to run into each other.
const OVERLAP: f64 = 0.2;

/// Below this share of the sheet inside its border, a drawing is not drawn
/// at the sheet's scale.
const FILLED: f64 = 0.12;

/// What is wrong with `drawing` as a sheet, each finding a sentence; empty
/// when nothing is.
pub fn review(drawing: &Drawing) -> Vec<String> {
    let tables = drawing.tables();
    let boxed: Vec<(EntityId, &Entity, Aabb)> = drawing
        .entities()
        // An xline or a ray has no box to be on a sheet with (ADR-0031).
        .filter(|(_, entity)| !entity.is_unbounded())
        .map(|(id, entity)| (id, entity, entity.bounds(tables)))
        .collect();
    let mut findings = Vec::new();
    findings.extend(overlapping_texts(&boxed));
    if let Some(sheet) = sheet(drawing, &boxed) {
        findings.extend(off_the_sheet(&sheet, &boxed));
        // A sheet with nothing drawn on it is one asked for alone — a title
        // block round views already drawn, which the review does not see:
        // only what is drawn on the sheet can be too small or undimensioned.
        let drawn = boxed
            .iter()
            .any(|(id, entity, _)| !sheet.members.contains(id) && words(entity).is_none());
        if !drawn {
            return findings;
        }
        findings.extend(sliver(&sheet, &boxed));
        let dimensioned = boxed
            .iter()
            .any(|(_, entity, _)| matches!(entity, Entity::Dimension(_)));
        if !dimensioned {
            findings.push(
                "The sheet has no dimensions: sizes are typed as text. Dimension the views with dim_linear (and dim_radius, dim_diameter), drawn at true size so the dimensions measure what they say."
                    .to_owned(),
            );
        }
    }
    findings
}

/// A document reviewed as a sheet of its own: run on an empty drawing in
/// `unit` (a suffix, `mm`), as the AI service does before it answers with
/// it. `None` when it does not run there — it acts on objects of a drawing
/// it was not given — so there is nothing to review.
pub fn review_document(intent: &cad_intent::Intent, unit: Option<&str>) -> Option<Vec<String>> {
    let mut session = crate::Session::new();
    session.resize(800.0, 600.0);
    if let Some(unit) = unit {
        session.submit_text("UNITS");
        session.submit_text(unit);
    }
    session.run_intent(intent).ok()?;
    Some(review(session.drawing()))
}

/// The words of a text, for a finding.
fn words(entity: &Entity) -> Option<&str> {
    match entity {
        Entity::Text(t) => Some(t.content.as_str()),
        Entity::MText(m) => Some(m.content.as_str()),
        _ => None,
    }
}

fn area(b: &Aabb) -> f64 {
    (b.max().x - b.min().x) * (b.max().y - b.min().y)
}

fn common(a: &Aabb, b: &Aabb) -> f64 {
    let w = a.max().x.min(b.max().x) - a.min().x.max(b.min().x);
    let h = a.max().y.min(b.max().y) - a.min().y.max(b.min().y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

fn inside(outer: &Aabb, inner: &Aabb) -> bool {
    let tolerance = 1e-6 * (area(outer).sqrt() + 1.0);
    inner.min().x >= outer.min().x - tolerance
        && inner.min().y >= outer.min().y - tolerance
        && inner.max().x <= outer.max().x + tolerance
        && inner.max().y <= outer.max().y + tolerance
}

fn overlapping_texts(boxed: &[(EntityId, &Entity, Aabb)]) -> Vec<String> {
    let texts: Vec<(&str, &Aabb)> = boxed
        .iter()
        .filter_map(|(_, entity, b)| Some((words(entity)?, b)))
        .collect();
    let mut pairs = Vec::new();
    for (i, (a, box_a)) in texts.iter().enumerate() {
        for (b, box_b) in &texts[i + 1..] {
            let smaller = area(box_a).min(area(box_b));
            if smaller > 0.0 && common(box_a, box_b) > OVERLAP * smaller {
                pairs.push(format!("\"{a}\" and \"{b}\""));
            }
        }
    }
    if pairs.is_empty() {
        return Vec::new();
    }
    let more = pairs.len().saturating_sub(SPELLED_OUT);
    pairs.truncate(SPELLED_OUT);
    let mut finding = format!("Texts run into each other: {}", pairs.join("; "));
    if more > 0 {
        finding.push_str(&format!("; and {more} more pairs"));
    }
    finding.push_str(". Move them apart, or set them smaller.");
    vec![finding]
}

/// The sheet a title block drew: the largest closed outline of its group,
/// and the members of the group, which are the sheet itself.
struct Sheet {
    edge: Aabb,
    members: Vec<EntityId>,
}

fn sheet(drawing: &Drawing, boxed: &[(EntityId, &Entity, Aabb)]) -> Option<Sheet> {
    let (_, group) = drawing
        .groups()
        .iter()
        .find(|(_, group)| group.name.starts_with(GROUP))?;
    let edge = boxed
        .iter()
        .filter(|(id, entity, _)| {
            group.members.contains(id) && matches!(entity, Entity::Polyline(p) if p.closed)
        })
        .map(|(_, _, b)| *b)
        .max_by(|a, b| area(a).total_cmp(&area(b)))?;
    Some(Sheet {
        edge,
        members: group.members.clone(),
    })
}

fn off_the_sheet(sheet: &Sheet, boxed: &[(EntityId, &Entity, Aabb)]) -> Vec<String> {
    let outside: Vec<String> = boxed
        .iter()
        .filter(|(id, _, b)| !sheet.members.contains(id) && !inside(&sheet.edge, b))
        .map(|(_, entity, _)| match words(entity) {
            Some(words) => format!("the text \"{words}\""),
            None => format!("a {}", entity.kind_name()),
        })
        .collect();
    if outside.is_empty() {
        return Vec::new();
    }
    let count = outside.len();
    let named: Vec<String> = outside.into_iter().take(SPELLED_OUT).collect();
    vec![format!(
        "{count} object{} lie{} outside the sheet ({}): place every view and note inside the border, or choose a larger sheet.",
        if count == 1 { "" } else { "s" },
        if count == 1 { "s" } else { "" },
        named.join(", ")
    )]
}

fn sliver(sheet: &Sheet, boxed: &[(EntityId, &Entity, Aabb)]) -> Vec<String> {
    let drawn = boxed
        .iter()
        // What is on the sheet: what lies off it is a finding of its own.
        .filter(|(id, _, b)| !sheet.members.contains(id) && inside(&sheet.edge, b))
        .map(|(_, _, b)| *b)
        .reduce(|a, b| a.union(&b));
    let Some(drawn) = drawn else {
        return Vec::new();
    };
    let share = area(&drawn) / area(&sheet.edge);
    if share >= FILLED {
        return Vec::new();
    }
    vec![format!(
        "The drawing fills only {:.0}% of the sheet. A title block at a scale enlarges the sheet: draw the views at true size inside it, never shrunk by the scale as well; or choose a smaller sheet or scale.",
        share * 100.0
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Harness;

    fn reviewed(lines: &[&str]) -> Vec<String> {
        let mut h = Harness::new();
        h.enter_all(&["UNITS", "mm"]);
        h.enter_all(lines);
        review(h.session.drawing())
    }

    /// What the service reviews: the document on an empty drawing; one that
    /// acts on objects it was not given is not reviewed.
    #[test]
    fn a_document_is_reviewed_on_an_empty_drawing() {
        let parse = |operations: &str| {
            cad_intent::parse(&format!(
                r#"{{"version": {}, "operations": [{operations}]}}"#,
                cad_intent::VERSION
            ))
            .unwrap()
        };
        let overlapping = parse(
            r#"{"op": "text", "at": [0, 0], "text": "SIDE ELEVATION", "height": 10},
               {"op": "text", "at": [20, 2], "text": "SCHEMATIC", "height": 10}"#,
        );
        let findings = review_document(&overlapping, Some("mm")).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        let elsewhere = parse(r#"{"op": "erase", "targets": [7]}"#);
        assert_eq!(review_document(&elsewhere, Some("mm")), None);
    }

    #[test]
    fn a_plain_drawing_without_a_sheet_has_nothing_to_say() {
        assert!(reviewed(&["LINE", "0,0", "100,0", ""]).is_empty());
    }

    #[test]
    fn texts_that_run_into_each_other_are_named() {
        let findings = reviewed(&[
            "TEXT",
            "0,0",
            "10",
            "0",
            "SIDE ELEVATION",
            "TEXT",
            "20,2",
            "10",
            "0",
            "SCHEMATIC",
        ]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("\"SIDE ELEVATION\" and \"SCHEMATIC\""),
            "{findings:?}"
        );
        assert!(
            reviewed(&[
                "TEXT", "0,0", "10", "0", "ONE", "TEXT", "0,100", "10", "0", "TWO"
            ])
            .is_empty()
        );
    }

    /// The Iron Man sheet: an A0 at 1:3, the figure shrunk by 3 as well,
    /// its sizes typed — and one note past the border.
    #[test]
    fn a_shrunk_undimensioned_drawing_on_a_scaled_sheet_is_found() {
        let mut h = Harness::new();
        h.enter_all(&[
            "UNITS",
            "mm",
            "TITLEBLOCK",
            "A0",
            "Landscape",
            "3",
            "ARMOR",
            "",
            "",
            "",
            "",
            "First",
            "0,0",
        ]);
        assert!(
            h.log().iter().any(|l| l.contains("drawn as TITLE-1")),
            "{:?}",
            h.log()
        );
        h.enter_all(&["RECTANG", "1000,500", "1200,1150"]);
        h.enter_all(&["TEXT", "-500,-500", "10", "0", "STRAY NOTE"]);
        let findings = review(h.session.drawing());
        let all = findings.join(" | ");
        assert!(all.contains("fills only"), "{all}");
        assert!(
            all.contains("outside the sheet (the text \"STRAY NOTE\")"),
            "{all}"
        );
        assert!(all.contains("no dimensions"), "{all}");

        // Drawn at true size and dimensioned, inside the border: nothing.
        let mut h = Harness::new();
        h.enter_all(&[
            "UNITS",
            "mm",
            "TITLEBLOCK",
            "A0",
            "Landscape",
            "3",
            "ARMOR",
            "",
            "",
            "",
            "",
            "First",
            "0,0",
        ]);
        h.enter_all(&["RECTANG", "300,300", "2800,2200"]);
        h.enter_all(&["DIMLINEAR", "300,300", "2800,300", "1500,200"]);
        assert_eq!(review(h.session.drawing()), Vec::<String>::new());
    }

    /// A title block asked for alone — round views the review does not see
    /// — is not faulted for being empty or undimensioned; a note on it is
    /// not a drawing either.
    #[test]
    fn a_sheet_asked_for_alone_is_not_faulted() {
        let mut h = Harness::new();
        h.enter_all(&[
            "UNITS",
            "mm",
            "TITLEBLOCK",
            "A3",
            "Landscape",
            "1",
            "BRACKET",
            "",
            "",
            "",
            "",
            "First",
            "-150,-140",
        ]);
        h.enter_all(&["TEXT", "0,0", "5", "0", "GENERAL NOTES"]);
        assert_eq!(review(h.session.drawing()), Vec::<String>::new());
    }
}
