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
    // Each text, and the figures of each dimension where its style lays
    // them out: a callout over a dimension's figures is as unreadable as
    // one over another text (#358).
    let mut texts: Vec<(String, Aabb)> = boxed
        .iter()
        .filter_map(|(_, entity, b)| Some((words(entity)?.to_owned(), *b)))
        .collect();
    for (_, entity, _) in &boxed {
        if let Entity::Dimension(dimension) = entity {
            let layout = dimension.layout(tables.dim_style(dimension.style));
            let figures = Entity::Text(layout.text.clone());
            texts.push((layout.text.content.clone(), figures.bounds(tables)));
        }
    }
    findings.extend(overlapping_texts(&texts));
    findings.extend(structure(drawing));
    findings.extend(on_layer_zero(drawing));
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
        findings.extend(too_small(drawing, &sheet, &boxed));
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

/// The scales a drafter gives a sheet, 1:N, in order.
const STANDARD_SCALES: [f64; 19] = [
    1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 75.0, 100.0, 125.0, 200.0, 250.0, 500.0, 1000.0, 1250.0,
    2000.0, 2500.0, 5000.0, 10000.0,
];

/// `intent` with its sheet on `paper` at 1:`to`, and a dimension scale
/// that was the sheet's moved with it.
fn rescaled(
    intent: &cad_intent::Intent,
    paper: cad_intent::PaperSize,
    from: f64,
    to: f64,
) -> cad_intent::Intent {
    use cad_intent::Operation;
    let mut tried = intent.clone();
    for op in &mut tried.operations {
        match op {
            Operation::TitleBlock {
                paper: on, scale, ..
            } => {
                *on = paper;
                *scale = Some(to);
            }
            Operation::DimScale { scale } if (*scale - from).abs() < 1e-9 => *scale = to,
            _ => {}
        }
    }
    tried
}

/// A document reviewed as a sheet of its own: run on an empty drawing in
/// `unit` (a suffix, `mm`), as the AI service does before it answers with
/// it. `None` when it does not run there — it acts on objects of a drawing
/// it was not given — so there is nothing to review.
pub fn review_document(intent: &cad_intent::Intent, unit: Option<&str>) -> Option<Vec<String>> {
    let run = |intent: &cad_intent::Intent| {
        let mut session = crate::Session::new();
        session.resize(800.0, 600.0);
        if let Some(unit) = unit {
            session.submit_text("UNITS");
            session.submit_text(unit);
        }
        session.run_intent(intent).map(|ran| (session, ran))
    };
    match run(intent) {
        // A hatch passed over is still the model's to put right (#358).
        Ok((session, ran)) => {
            let mut findings = review(session.drawing());
            findings.extend(ran.iter().filter_map(|r| r.skipped.as_ref()).map(|why| {
                format!(
                    "HATCH was skipped, {why}: draw the closed outline it fills in that view, its point well inside it, or leave the hatch out."
                )
            }));
            Some(findings)
        }
        // A sheet that cannot be composed — a view with no room — is the
        // document's own layout, which a revision can put right; any other
        // refusal is the program's answer to the request, not a finding.
        // The smallest paper it composes on, the rest the same, is said.
        Err(crate::intent::RunError::Refused {
            command, reason, ..
        }) if COMPOSING.contains(&command) => {
            use cad_intent::{Operation, PaperSize};
            let larger = |paper: PaperSize| {
                let order = [
                    PaperSize::A4,
                    PaperSize::A3,
                    PaperSize::A2,
                    PaperSize::A1,
                    PaperSize::A0,
                ];
                let at = order.iter().position(|p| *p == paper).unwrap_or(0);
                order[at + 1..].to_vec()
            };
            let current = intent.operations.iter().find_map(|op| match op {
                Operation::TitleBlock { paper, .. } => Some(*paper),
                _ => None,
            });
            // Only a sheet without room is told of paper and scale.
            // A view at its own scale is told in the reason what fits:
            // the sheet's paper and scale do not size it; nor does any
            // scale a view whose text and dimensions are its size (#358).
            let room = reason.contains("there is not that much room")
                && !reason.contains("its own scale alone sets its size")
                && !reason.contains("a smaller scale would not make room");
            let fits = current.filter(|_| room).and_then(|paper| {
                larger(paper).into_iter().find(|bigger| {
                    let mut tried = intent.clone();
                    for op in &mut tried.operations {
                        if let Operation::TitleBlock { paper, .. } = op {
                            *paper = *bigger;
                        }
                    }
                    run(&tried).is_ok()
                })
            });
            // No paper large enough at this scale: the first standard scale,
            // on this paper or on A0, at which it does compose — a number to
            // give back, as a detail is told the ratio that fits (#358).
            let scaled = (room && fits.is_none())
                .then(|| {
                    let (paper, scale) = intent.operations.iter().find_map(|op| match op {
                        Operation::TitleBlock { paper, scale, .. } => {
                            Some((*paper, scale.unwrap_or(1.0)))
                        }
                        _ => None,
                    })?;
                    let papers = if paper == PaperSize::A0 {
                        vec![paper]
                    } else {
                        vec![paper, PaperSize::A0]
                    };
                    papers.into_iter().find_map(|on| {
                        STANDARD_SCALES
                            .iter()
                            .copied()
                            .filter(|n| *n > scale)
                            .find(|n| run(&rescaled(intent, on, scale, *n)).is_ok())
                            .map(|n| (on, n))
                    })
                })
                .flatten();
            let reason = reason.trim_end_matches('.');
            let advice = match (fits, scaled) {
                (Some(paper), _) => format!(
                    " At this scale it composes on {paper:?}: use {paper:?}, or a smaller scale, narrower notes, or fewer views."
                ),
                (None, Some((paper, n))) => format!(
                    " Not even A0 has room at this scale. On {paper:?} it composes at 1:{n}: give the title block `paper` {paper:?} and `scale` {n}, and `dim_scale` {n}."
                ),
                (None, None) if room => " Not even A0 has room at this scale: choose a smaller scale, narrower notes, or fewer views.".to_owned(),
                (None, None) => String::new(),
            };
            Some(vec![format!(
                "The sheet could not be composed: {reason}.{advice}"
            )])
        }
        // So is what a view's own drawing refuses — a hatch with no closed
        // boundary: the view's geometry, which the model draws again.
        Err(crate::intent::RunError::Refused {
            operation,
            command,
            reason,
        }) if matches!(
            intent.operations.get(operation),
            Some(cad_intent::Operation::View { .. })
        ) =>
        {
            let id = match &intent.operations[operation] {
                cad_intent::Operation::View { id, .. } => id.clone(),
                _ => String::new(),
            };
            let reason = reason.trim_end_matches('.');
            Some(vec![format!(
                "View {id} could not be drawn: {command} refused: {reason}. Draw the view's geometry so that it does what is asked of it."
            )])
        }
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
    // A projected view shows the object's height (from a side or behind)
    // or its width (from above or below) as its parent does: its outline —
    // not its text, dimensions, centre or hidden lines, balloons, or
    // markers — spans the same (#358).
    let outline = |view: &crate::compose::ViewRecord| {
        let members = group(&format!("{VIEW_GROUP}{}", view.id)).unwrap_or_default();
        let skipped = [crate::compose::SECTION_LAYER, crate::bom::BALLOON_LAYER]
            .map(|name| drawing.layers().find(name));
        members
            .iter()
            .filter_map(|id| Some((drawing.entity(*id)?, drawing.record(*id)?)))
            .filter(|(e, record)| {
                let layer = drawing.layers().get(record.properties.layer);
                let linetype = match record.properties.linetype {
                    cad_document::Linetype::ByLayer => {
                        layer.map_or(cad_document::Linetype::CONTINUOUS, |l| l.linetype)
                    }
                    other => other,
                };
                !matches!(
                    e,
                    Entity::Text(_) | Entity::MText(_) | Entity::Dimension(_) | Entity::Hatch(_)
                ) && (linetype == cad_document::Linetype::ByBlock
                    || linetype == cad_document::Linetype::CONTINUOUS)
                    && !skipped.contains(&Some(record.properties.layer))
            })
            .map(|(e, _)| e.bounds(tables))
            .reduce(|a, b| a.union(&b))
    };
    for view in &views {
        let Role::Projected { of, looking } = &view.role else {
            continue;
        };
        let Some(parent) = views.iter().find(|v| &v.id == of) else {
            continue;
        };
        let (Some(here), Some(there)) = (outline(view), outline(parent)) else {
            continue;
        };
        let (axis, a, b) = match looking {
            Looking::Above | Looking::Below => (
                "width",
                here.max().x - here.min().x,
                there.max().x - there.min().x,
            ),
            _ => (
                "height",
                here.max().y - here.min().y,
                there.max().y - there.min().y,
            ),
        };
        if (a - b).abs() > 0.005 * a.max(b) + tolerance {
            findings.push(format!(
                "Views {} and {of} disagree on the object's {axis}: {} in {} and {} in {of}. A projected view shows the whole object, as tall and as wide as the view it is projected from: draw both from the same positions.",
                view.id,
                number(a),
                view.id,
                number(b)
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

/// A length as a finding says it: to the hundredth, no trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
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

fn overlapping_texts(texts: &[(String, Aabb)]) -> Vec<String> {
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

/// Text too small to read on paper (#358): under 1.8 mm once the sheet's
/// scale is taken out of its height. Said with the height on paper and
/// the height that would read.
fn too_small(drawing: &Drawing, sheet: &Sheet, boxed: &[(EntityId, &Entity, Aabb)]) -> Vec<String> {
    let Some(frame) = crate::compose::sheet_of(drawing).map(|s| s.frame) else {
        return Vec::new();
    };
    let per_mm = frame.units_per_mm;
    let small: Vec<(String, f64)> = boxed
        .iter()
        .filter(|(id, _, _)| !sheet.members.contains(id))
        .filter_map(|(_, entity, _)| {
            let (words, height) = match entity {
                Entity::Text(t) => (t.content.as_str(), t.height),
                Entity::MText(m) => (m.content.as_str(), m.height),
                _ => return None,
            };
            let on_paper = height / per_mm;
            (on_paper < SMALLEST_TEXT).then(|| (words.to_owned(), on_paper))
        })
        .collect();
    if small.is_empty() {
        return Vec::new();
    }
    let named: Vec<String> = small
        .iter()
        .take(SPELLED_OUT)
        .map(|(words, mm)| format!("\"{words}\" {mm:.1} mm"))
        .collect();
    let more = small.len().saturating_sub(SPELLED_OUT);
    let and_more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    vec![format!(
        "Texts too small to read on paper: {}{and_more}. Give each at least 2.5 mm on paper — a `height` of {} at this sheet's scale.",
        named.join(", "),
        number(2.5 * per_mm)
    )]
}

/// Text under this height on paper, in millimetres, cannot be read.
const SMALLEST_TEXT: f64 = 1.8;

/// The objects of a sheet's views left on layer 0 when the drawing has
/// layers of its own for them (#358): stirrups drawn before their layer
/// was made current, beside the one that was. Said by kind, so the model
/// can tell which.
fn on_layer_zero(drawing: &Drawing) -> Vec<String> {
    use crate::compose::{VIEW_GROUP, sheet_of};
    let Some(sheet) = sheet_of(drawing) else {
        return Vec::new();
    };
    let zero = drawing.layers().find(cad_document::Layer::ZERO);
    // Layers of the drawing's own: not 0, and not those the sheet's
    // machinery is drawn on.
    let machinery = [
        crate::titleblock::LAYER,
        crate::compose::TITLE_LAYER,
        crate::compose::SECTION_LAYER,
        crate::bom::TABLE_LAYER,
        crate::bom::BALLOON_LAYER,
        crate::commands::NOTES_LAYER,
    ];
    let own_layers = drawing.layers().iter().any(|(_, layer)| {
        layer.name != cad_document::Layer::ZERO && !machinery.contains(&layer.name.as_str())
    });
    if zero.is_none() || !own_layers {
        return Vec::new();
    }
    let mut kinds: Vec<(&'static str, usize)> = Vec::new();
    for (_, group) in drawing.groups().iter() {
        let of_sheet = serde_json::from_str::<serde_json::Value>(&group.description)
            .is_ok_and(|v| v["sheet"].as_str() == Some(sheet.group.as_str()));
        if !group.name.starts_with(VIEW_GROUP) || !of_sheet {
            continue;
        }
        for id in &group.members {
            let (Some(entity), Some(record)) = (drawing.entity(*id), drawing.record(*id)) else {
                continue;
            };
            if Some(record.properties.layer) != zero {
                continue;
            }
            let kind = entity.kind_name();
            match kinds.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, n)) => *n += 1,
                None => kinds.push((kind, 1)),
            }
        }
    }
    let total: usize = kinds.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return Vec::new();
    }
    let listed: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
    vec![format!(
        "{total} objects of the views are on layer 0 ({}), beside layers made for them: make each kind's layer current with `layer` before drawing it, or move them onto it with `set_properties`.",
        listed.join(", ")
    )]
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
    /// The maintainer's house (#358): a 7 × 15 m ground floor at 1:50 and
    /// a 10 × 30 m site inset at 1:200 on A2 portrait take 140 × 300 mm and
    /// 50 × 150 mm of a 420 × 594 mm sheet, and compose with nothing to
    /// say. An inset made large by its labels is told so, measured — its
    /// drawing's size and its tallest text on paper — and never sent to a
    /// smaller scale or a larger sheet, which would not make room.
    #[test]
    fn a_site_inset_at_its_own_scale_fits_and_one_too_large_says_what_makes_it() {
        let house = |label_height: u32| {
            cad_intent::parse(&format!(
                r#"{{"version": {}, "operations": [
                   {{"op": "title_block", "paper": "A2", "portrait": true, "scale": 50, "title": "HOUSE", "at": [0, 0]}},
                   {{"op": "dim_scale", "scale": 50}},
                   {{"op": "view", "id": "plan", "placed": "principal", "title": "GROUND FLOOR PLAN", "operations": [
                       {{"op": "rectangle", "corner": [1500, 5000], "opposite": [8500, 20000]}},
                       {{"op": "rectangle", "corner": [1700, 5200], "opposite": [8300, 19800]}},
                       {{"op": "dim_linear", "from": [1500, 5000], "to": [8500, 5000], "at": [5000, 3800]}},
                       {{"op": "dim_linear", "from": [1500, 5000], "to": [1500, 20000], "at": [300, 12500]}},
                       {{"op": "text", "at": [3000, 7000], "height": 125, "text": "LIVING"}}]}},
                   {{"op": "view", "id": "site", "placed": "free", "looking": "from_above", "scale": "1:200", "title": "SITE PLAN", "operations": [
                       {{"op": "rectangle", "corner": [0, 0], "opposite": [10000, 30000]}},
                       {{"op": "rectangle", "corner": [1500, 5000], "opposite": [8500, 20000]}},
                       {{"op": "segments", "segments": [[[1500, 0], [1500, 30000]], [[8500, 0], [8500, 30000]], [[0, 5000], [10000, 5000]], [[0, 20000], [10000, 20000]]]}},
                       {{"op": "text", "at": [2500, 24000], "height": {label_height}, "text": "GARDEN / PATIO"}},
                       {{"op": "dim_linear", "from": [0, 0], "to": [10000, 0], "at": [5000, -1500]}},
                       {{"op": "dim_linear", "from": [10000, 0], "to": [10000, 30000], "at": [11500, 15000]}}]}}]}}"#,
                cad_intent::VERSION
            ))
            .unwrap()
        };
        assert_eq!(review_document(&house(125), Some("mm")), Some(vec![]));
        // A label 60 mm high on paper, as if the inset's scale sized it.
        let findings = review_document(&house(3000), Some("mm")).unwrap();
        assert!(
            findings[0].contains("View site, at its own scale 1:200, needs")
                && findings[0].contains("yet its drawing at that scale is only 50 × 150 mm")
                && findings[0].contains("\"GARDEN / PATIO\", is 60 mm high on paper")
                && findings[0].contains("Keep the scale")
                && !findings[0].contains("or smaller fits")
                && !findings[0].contains("composes on"),
            "{findings:?}"
        );
    }

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
            findings[0].starts_with("The sheet could not be composed: View front needs")
                && findings[0].ends_with(
                    "At this scale it composes on A1: use A1, or a smaller scale, narrower notes, or fewer views."
                ),
            "{findings:?}"
        );
        let parallel = parse(
            r#"{"op": "line", "points": [[0, 0], [10, 0]]},
               {"op": "line", "points": [[0, 5], [10, 5]]},
               {"op": "fillet", "radius": 1, "first": {"id": 1, "near": [5, 0]}, "second": {"id": 2, "near": [5, 5]}}"#,
        );
        assert_eq!(review_document(&parallel, Some("mm")), None);
        // A hatch in a view with no outline around it is skipped, and the
        // review still says so, for the model to draw the outline.
        let open = parse(
            r#"{"op": "title_block", "paper": "A3", "scale": 1, "title": "X", "at": [0, 0]},
               {"op": "view", "id": "front", "placed": "principal", "operations": [
                   {"op": "line", "points": [[0, 0], [50, 0], [50, 30]]},
                   {"op": "hatch", "pattern": "ANSI31", "inside": [25, 10]}]}"#,
        );
        let findings = review_document(&open, Some("mm")).unwrap();
        assert!(
            findings.iter().any(|f| f.starts_with(
                "HATCH was skipped, a hatch in view front: no closed outline surrounds its point"
            )),
            "{findings:?}"
        );
        // A plan too large for A0 at 1:20, as a culvert's was (#358): told
        // the first standard scale at which it composes, not only to choose
        // a smaller one.
        let culvert = parse(
            r#"{"op": "title_block", "paper": "A0", "scale": 20, "title": "H01", "at": [0, 0]},
               {"op": "dim_scale", "scale": 20},
               {"op": "view", "id": "plan", "placed": "free", "looking": "from_above", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [18000, 18400]}]}"#,
        );
        let findings = review_document(&culvert, Some("mm")).unwrap();
        assert!(
            findings[0].contains("Not even A0 has room at this scale. On A0 it composes at 1:25"),
            "{findings:?}"
        );
        // A detail too large at its own scale is told the ratio that fits,
        // and nothing of the sheet's scale, which does not size it.
        let detail = parse(
            r#"{"op": "title_block", "paper": "A3", "scale": 100, "title": "X", "at": [0, 0]},
               {"op": "view", "id": "front", "placed": "principal", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [20000, 5000]}]},
               {"op": "view", "id": "sill", "placed": "free", "ratio": 2, "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [500, 1000]}]}"#,
        );
        let findings = review_document(&detail, Some("mm")).unwrap();
        assert!(
            findings[0].contains("at its own scale 2:1")
                && findings[0].ends_with("A `scale` of \"1:10\" or smaller fits."),
            "{findings:?}"
        );
        // A refusal of the sheet that is not about room is told as it is,
        // not with paper and scale.
        let unlisted = parse(
            r#"{"op": "title_block", "paper": "A3", "scale": 1, "title": "X", "at": [0, 0]},
               {"op": "view", "id": "front", "placed": "principal", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [50, 30]}]},
               {"op": "balloon", "part": 9, "view": "front", "at": [25, 30], "to": [60, 50]}"#,
        );
        let findings = review_document(&unlisted, Some("mm")).unwrap();
        assert!(
            findings[0].starts_with("The sheet could not be composed: ")
                && !findings[0].contains("scale")
                && !findings[0].contains(".."),
            "{findings:?}"
        );
    }

    /// What the beam and spillway sheets showed (#358): a label 0.5 mm high
    /// on paper, stirrups left on layer 0 beside the layer made for them, a
    /// callout over a dimension's figures — each a finding — and a detail
    /// whose title says its scale already, not said twice.
    #[test]
    fn small_texts_layer_zero_and_callouts_on_dimensions_are_found() {
        let document = cad_intent::parse(&format!(
            r#"{{"version": {}, "operations": [
                {{"op": "title_block", "paper": "A1", "scale": 100, "title": "B", "at": [0, 0]}},
                {{"op": "layer", "name": "OUTLINE", "lineweight": "0.5"}},
                {{"op": "view", "id": "front", "placed": "principal", "operations": [
                    {{"op": "layer", "name": "OUTLINE", "current": true}},
                    {{"op": "rectangle", "corner": [0, 0], "opposite": [20000, 6000]}},
                    {{"op": "layer", "name": "0", "current": true}},
                    {{"op": "rectangle", "corner": [1000, 1000], "opposite": [2000, 5000]}},
                    {{"op": "dim_linear", "from": [0, 0], "to": [20000, 0], "orientation": "horizontal", "at": [10000, -1500]}},
                    {{"op": "text", "at": [9000, -1450], "height": 300, "text": "BARS 3 PHI 20"}},
                    {{"op": "text", "at": [500, 3000], "height": 50, "text": "COVER 30"}}]}},
                {{"op": "view", "id": "detail", "title": "DETAIL (2:1)", "placed": "free", "ratio": 2, "operations": [
                    {{"op": "rectangle", "corner": [0, 0], "opposite": [100, 100]}}]}}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap();
        let findings = review_document(&document, Some("mm")).unwrap();
        let found = |words: &str| findings.iter().any(|f| f.contains(words));
        assert!(
            found("Texts too small to read on paper: \"COVER 30\" 0.5 mm"),
            "{findings:?}"
        );
        assert!(found("a `height` of 250"), "{findings:?}");
        assert!(
            found("5 objects of the views are on layer 0 (2 polyline, 1 dimension, 2 text)"),
            "{findings:?}"
        );
        assert!(found("\"BARS 3 PHI 20\" and \"20000"), "{findings:?}");
        // The detail's title as written, its scale said once.
        let mut session = crate::Session::new();
        session.submit_text("UNITS");
        session.submit_text("mm");
        session.run_intent(&document).unwrap();
        let titles: Vec<String> = session
            .drawing()
            .entities()
            .filter_map(|(_, e)| match e {
                Entity::Text(t) if t.content.starts_with("DETAIL") => Some(t.content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["DETAIL (2:1)"]);
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
