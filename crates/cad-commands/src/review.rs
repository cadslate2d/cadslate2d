//! A drawing's review (#349): what is plainly wrong with a sheet a request
//! made, said so it can be put right before the proposal is shown. Words
//! that run into each other; anything off the sheet; a drawing that fills a
//! sliver of its sheet, as one shrunk to a scale the sheet was already
//! enlarged by; a sheet whose sizes are typed instead of dimensioned. Each
//! finding is a sentence a model can act on. Nothing here changes the
//! drawing.

use cad_core::{Aabb, EntityId, Point2};
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
    findings.extend(structure(drawing));
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

/// The commands that compose a sheet: what they refuse is the layout.
const COMPOSING: [&str; 4] = ["VIEWFRAME", "BOMTABLE", "NOTES", "BALLOON"];

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
    match session.run_intent(intent) {
        Ok(_) => Some(review(session.drawing())),
        // A sheet that cannot be composed — a view with no room — is the
        // document's own layout, which a revision can put right; any other
        // refusal is the program's answer to the request, not a finding.
        Err(crate::intent::RunError::Refused {
            command, reason, ..
        }) if COMPOSING.contains(&command) => Some(vec![format!(
            "The sheet could not be composed: {reason}. Give the views, notes, and table room on the sheet: a larger paper or scale, narrower notes, or fewer views."
        )]),
        Err(_) => None,
    }
}

/// What is wrong with a composed sheet's structure (ADR-0041 §7, #357):
/// a view's objects outside its box, views overlapping, a projected view
/// out of line with its parent, a section without its marker, a balloon
/// for a part no bill has. A view moved by hand is where its title went.
fn structure(drawing: &Drawing) -> Vec<String> {
    use crate::compose::{SECTION_GROUP, VIEW_GROUP, parts_on, sheet_of, views_on};
    use cad_geometry::composition::{Looking, Role};
    let Some(sheet) = sheet_of(drawing) else {
        return Vec::new();
    };
    let tables = drawing.tables();
    let views = views_on(drawing, &sheet.group);
    let group = |name: &str| {
        drawing
            .groups()
            .iter()
            .find(|(_, g)| g.name == name)
            .map(|(_, g)| g.members.clone())
    };
    let tolerance = 1e-6 * (area(&sheet.frame.border).sqrt() + 1.0);
    // Each view where it is now: moved as far as its title was.
    let moved: Vec<(String, cad_core::Vec2, Aabb)> = views
        .iter()
        .map(|view| {
            let members = group(&format!("{VIEW_GROUP}{}", view.id)).unwrap_or_default();
            let title = members.iter().find_map(|id| match drawing.entity(*id) {
                Some(Entity::Text(t)) if t.content == view.title => Some(t.insertion),
                _ => None,
            });
            let by = match (title, view.title_at) {
                (Some(now), Some(then)) => now - then,
                _ => cad_core::Vec2::new(0.0, 0.0),
            };
            let at = view.origin + by;
            let b = Aabb::from_points(
                at + (view.extent.min() - Point2::ORIGIN),
                at + (view.extent.max() - Point2::ORIGIN),
            );
            (view.id.clone(), by, b)
        })
        .collect();
    let mut findings = Vec::new();
    for (view, (_, _, b)) in views.iter().zip(&moved) {
        let members = group(&format!("{VIEW_GROUP}{}", view.id)).unwrap_or_default();
        let outside = members.iter().any(|id| {
            drawing.entity(*id).is_some_and(|e| {
                let e = e.bounds(tables);
                e.min().x < b.min().x - tolerance
                    || e.min().y < b.min().y - tolerance
                    || e.max().x > b.max().x + tolerance
                    || e.max().y > b.max().y + tolerance
            })
        });
        if outside {
            findings.push(format!(
                "View {} has objects outside its frame: keep a view's drawing inside it, or draw the view again.",
                view.id
            ));
        }
    }
    for (i, (a, _, box_a)) in moved.iter().enumerate() {
        for (b, _, box_b) in &moved[i + 1..] {
            if common(box_a, box_b) > 0.0 {
                findings.push(format!("Views {a} and {b} overlap: place them apart."));
            }
        }
    }
    for (view, (_, by, _)) in views.iter().zip(&moved) {
        let Role::Projected { of, looking } = &view.role else {
            continue;
        };
        let Some(parent) = views.iter().position(|v| &v.id == of) else {
            continue;
        };
        let here = view.origin + *by;
        let there = views[parent].origin + moved[parent].1;
        let off = match looking {
            Looking::Above | Looking::Below => (here.x - there.x).abs(),
            _ => (here.y - there.y).abs(),
        };
        if off > tolerance {
            findings.push(format!(
                "View {} is out of line with view {of}, which it is projected from: move it back in line.",
                view.id
            ));
        }
    }
    for view in &views {
        if let Some((letter, _)) = &view.section {
            let marked = group(&format!("{SECTION_GROUP}{letter}"))
                .is_some_and(|members| members.iter().any(|id| drawing.entity(*id).is_some()));
            if !marked {
                findings.push(format!(
                    "Section {letter}-{letter} has no cutting line marked: draw the section again."
                ));
            }
        }
    }
    let parts: Vec<u32> = parts_on(drawing, &sheet.group)
        .iter()
        .map(|p| p.number)
        .collect();
    for (_, g) in drawing.groups().iter() {
        if !g.name.starts_with(crate::bom::BALLOON_GROUP) {
            continue;
        }
        let part = serde_json::from_str::<serde_json::Value>(&g.description)
            .ok()
            .and_then(|v| v["part"].as_u64());
        if let Some(part) = part
            && !parts.contains(&(part as u32))
        {
            findings.push(format!(
                "A balloon names part {part}, which no bill of materials on the sheet has."
            ));
        }
    }
    findings
}

/// A document's review from its text, as the model wrote it: the drawn
/// sheet's findings ([`review_document`]) and its datums declared for views
/// that do not draw them (ADR-0041 §7). `None` when it is no document, or
/// does not run on an empty drawing.
pub fn review_text(text: &str, unit: Option<&str>) -> Option<Vec<String>> {
    let intent = cad_intent::parse(text).ok()?;
    let mut findings = review_document(&intent, unit)?;
    findings.extend(cad_intent::unused_datums(text));
    Some(findings)
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

    /// A sheet that cannot be composed is a finding, for the model to lay
    /// out again; a refusal of another kind is not.
    #[test]
    fn a_sheet_with_no_room_is_a_finding_and_other_refusals_are_not() {
        let parse = |operations: &str| {
            cad_intent::parse(&format!(
                r#"{{"version": {}, "operations": [{operations}]}}"#,
                cad_intent::VERSION
            ))
            .unwrap()
        };
        let crowded = parse(
            r#"{"op": "title_block", "paper": "A4", "scale": 1, "title": "X", "at": [0, 0]},
               {"op": "view", "id": "front", "placed": "principal", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [600, 400]}]}"#,
        );
        let findings = review_document(&crowded, Some("mm")).unwrap();
        assert!(
            findings[0].starts_with("The sheet could not be composed: View front needs"),
            "{findings:?}"
        );
        let parallel = parse(
            r#"{"op": "line", "points": [[0, 0], [10, 0]]},
               {"op": "line", "points": [[0, 5], [10, 5]]},
               {"op": "fillet", "radius": 1, "first": {"id": 1, "near": [5, 0]}, "second": {"id": 2, "near": [5, 5]}}"#,
        );
        assert_eq!(review_document(&parallel, Some("mm")), None);
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
