//! A request's requirements checked on the drawing (#362): each the model
//! wrote beside its document, in the request's words, checked here against
//! what was drawn — a view at its scale, a dimension's reading, a text, a
//! number of outlines of a size, a layer — and said met or not, with what
//! was found instead. An unmet one goes back to the model; every one is
//! shown to the user with the answer.

use cad_core::{Aabb, EntityId};
use cad_document::{Drawing, Entity};
use cad_intent::{Check, Intent, Requirement};

use crate::compose::{sheet_of, view_group, views_on};

/// A requirement, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The requirement in the request's words.
    pub says: String,
    pub met: bool,
    /// For one not met, what the drawing has instead.
    pub found: String,
}

/// Two lengths are one when they agree to a millionth of the larger, and
/// never closer than a thousandth of a unit.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= (1e-6 * a.abs().max(b.abs())).max(1e-3)
}

fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The objects a check looks at: the view `id`'s, or the drawing's.
fn scope(drawing: &Drawing, view: Option<&str>) -> Result<Vec<EntityId>, String> {
    let Some(id) = view else {
        return Ok(drawing.entities().map(|(id, _)| id).collect());
    };
    let sheet = sheet_of(drawing).ok_or_else(|| format!("there is no sheet, so no view {id}"))?;
    let name = view_group(&sheet.group, id);
    drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == name)
        .map(|(_, g)| g.members.clone())
        .ok_or_else(|| format!("there is no view {id}"))
}

/// What a dimension reads: its length through its style's factor.
fn reading(drawing: &Drawing, id: EntityId) -> Option<f64> {
    match drawing.entity(id)? {
        Entity::Dimension(d) => {
            let style = drawing.tables().dim_style(d.style);
            Some(d.value() * style.linear_factor)
        }
        _ => None,
    }
}

fn words(entity: &Entity) -> Option<&str> {
    match entity {
        Entity::Text(t) => Some(&t.content),
        Entity::MText(t) => Some(&t.content),
        _ => None,
    }
}

/// `requirement` checked on `drawing`, its lengths `factor` drawing units
/// each (the document's units to the drawing's).
fn check_one(drawing: &Drawing, requirement: &Requirement, factor: f64) -> Outcome {
    let outcome = |met: bool, found: String| Outcome {
        says: requirement.says.clone(),
        met,
        found: if met { String::new() } else { found },
    };
    match &requirement.check {
        Check::View { id, scale } => {
            let Some(sheet) = sheet_of(drawing) else {
                return outcome(false, format!("there is no sheet, so no view {id}"));
            };
            let Some(view) = views_on(drawing, &sheet.group)
                .into_iter()
                .find(|v| &v.id == id)
            else {
                return outcome(false, format!("there is no view {id}"));
            };
            let Some(wanted) = scale
                .as_deref()
                .and_then(|s| crate::stated::stated(s).scales.first().copied())
            else {
                return outcome(true, String::new());
            };
            // What the view is drawn at: its own scale, or the sheet's.
            let drawn = view.ratio.or_else(|| {
                drawing
                    .groups()
                    .iter()
                    .find(|(_, g)| g.name == sheet.group)
                    .and_then(|(_, g)| {
                        serde_json::from_str::<serde_json::Value>(&g.description).ok()
                    })
                    .and_then(|r| {
                        crate::stated::stated(r["scale"].as_str()?)
                            .scales
                            .first()
                            .copied()
                    })
            });
            match drawn {
                Some(d) if (d / wanted - 1.0).abs() < 1e-9 => outcome(true, String::new()),
                Some(d) => outcome(
                    false,
                    format!("view {id} is drawn at {}", crate::compose::ratio_text(d)),
                ),
                None => outcome(false, format!("view {id}'s scale cannot be read")),
            }
        }
        Check::Dimension { value, view } => {
            let ids = match scope(drawing, view.as_deref()) {
                Ok(ids) => ids,
                Err(why) => return outcome(false, why),
            };
            let wanted = value * factor;
            let readings: Vec<f64> = ids.iter().filter_map(|id| reading(drawing, *id)).collect();
            if readings.iter().any(|r| same(*r, wanted)) {
                return outcome(true, String::new());
            }
            let nearest = readings
                .iter()
                .copied()
                .min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()));
            let place = view
                .as_deref()
                .map_or_else(String::new, |v| format!(" in view {v}"));
            outcome(
                false,
                match nearest {
                    Some(n) => format!(
                        "no dimension{place} reads {}; the nearest reads {}",
                        number(*value),
                        number(n / factor)
                    ),
                    None => format!("there is no dimension{place}"),
                },
            )
        }
        Check::Text { contains, view } => {
            let ids = match scope(drawing, view.as_deref()) {
                Ok(ids) => ids,
                Err(why) => return outcome(false, why),
            };
            let wanted = contains.to_lowercase();
            let met = ids.iter().any(|id| {
                drawing
                    .entity(*id)
                    .and_then(words)
                    .is_some_and(|w| w.to_lowercase().contains(&wanted))
            });
            let place = view
                .as_deref()
                .map_or_else(String::new, |v| format!(" in view {v}"));
            outcome(met, format!("no text{place} says \"{contains}\""))
        }
        Check::Outlines {
            count,
            width,
            height,
            view,
        } => {
            let ids = match scope(drawing, view.as_deref()) {
                Ok(ids) => ids,
                Err(why) => return outcome(false, why),
            };
            let (w, h) = (width * factor, height * factor);
            let tables = drawing.tables();
            let found = ids
                .iter()
                .filter_map(|id| match drawing.entity(*id)? {
                    entity @ Entity::Polyline(p) if p.closed => Some(entity.bounds(tables)),
                    // A hole is an outline as much as a block is.
                    entity @ Entity::Circle(_) => Some(entity.bounds(tables)),
                    _ => None,
                })
                .filter(|b: &Aabb| {
                    let (bw, bh) = (b.max().x - b.min().x, b.max().y - b.min().y);
                    (same(bw, w) && same(bh, h)) || (same(bw, h) && same(bh, w))
                })
                .count();
            let place = view
                .as_deref()
                .map_or_else(String::new, |v| format!(" in view {v}"));
            outcome(
                found == *count as usize,
                format!(
                    "{found} closed outlines {} × {} are drawn{place}, not {count}",
                    number(*width),
                    number(*height)
                ),
            )
        }
        Check::Layer { name } => {
            let Some(layer) = drawing.layers().find(name) else {
                return outcome(false, format!("there is no layer {name}"));
            };
            let used = drawing
                .entities()
                .any(|(id, _)| drawing.properties(id).is_some_and(|p| p.layer == layer));
            outcome(used, format!("layer {name} has nothing on it"))
        }
    }
}

/// Each of `requirements` checked on `drawing`, its lengths `factor`
/// drawing units each.
pub fn check(drawing: &Drawing, requirements: &[Requirement], factor: f64) -> Vec<Outcome> {
    requirements
        .iter()
        .map(|r| check_one(drawing, r, factor))
        .collect()
}

/// `requirements` checked on `intent` drawn on an empty drawing in `unit`
/// (a suffix, `mm`), as the review draws it; `None` when it does not run
/// there.
pub fn check_document(
    intent: &Intent,
    requirements: &[Requirement],
    unit: Option<&str>,
) -> Option<Vec<Outcome>> {
    let mut session = crate::Session::new();
    session.resize(800.0, 600.0);
    if let Some(unit) = unit {
        session.submit_text("UNITS");
        session.submit_text(unit);
    }
    session.run_intent(intent).ok()?;
    let drawing = session.drawing();
    let factor = match (intent.units, drawing.unit()) {
        (Some(from), Some(to)) => from.length_unit().metres() / to.metres(),
        _ => 1.0,
    };
    Some(check(drawing, requirements, factor))
}

/// An unmet requirement as a finding the model can act on.
pub fn finding(outcome: &Outcome) -> String {
    format!(
        "The request requires \"{}\", and the drawing does not do it: {}. Draw it so that it does — or, if it cannot be done as asked, say so in `reply`.",
        outcome.says, outcome.found
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spillway's sheet as a model might write it, in metres on a drawing
    /// in millimetres.
    fn sheet(bb_scale: &str) -> Intent {
        cad_intent::parse(&format!(
            r#"{{"version": {}, "units": "m", "operations": [
               {{"op": "title_block", "paper": "A1", "scale": 100, "title": "H02", "at": [0, 0]}},
               {{"op": "dim_scale", "scale": 100}},
               {{"op": "view", "id": "plan", "placed": "principal", "title": "TOP PLAN", "operations": [
                   {{"op": "rectangle", "corner": [0, 0], "opposite": [44, 10]}},
                   {{"op": "layer", "name": "HATCH", "current": true}},
                   {{"op": "rectangle", "corner": [28, 1.5], "opposite": [29, 2.5]}},
                   {{"op": "rectangle", "corner": [28, 4.5], "opposite": [29, 5.5]}},
                   {{"op": "rectangle", "corner": [28, 7.5], "opposite": [29, 8.5]}},
                   {{"op": "dim_linear", "from": [0, 0], "to": [0, 10], "at": [-2, 5]}},
                   {{"op": "text", "at": [40, 11], "height": 0.25, "text": "SILL EL+93.000"}}]}},
               {{"op": "view", "id": "bb", "placed": "section", "title": "SECTION B-B"{bb_scale}, "cut": {{"of": "plan", "from": [28.5, -1], "to": [28.5, 11], "toward": [40, 5]}}, "operations": [
                   {{"op": "rectangle", "corner": [-0.6, 0], "opposite": [10.6, 1]}}]}}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap()
    }

    fn requirement(says: &str, check: &str) -> Requirement {
        Requirement {
            says: says.to_owned(),
            check: serde_json::from_str(check).unwrap(),
        }
    }

    #[test]
    fn what_was_drawn_meets_its_requirements_and_what_was_not_says_what_is_there() {
        let wanted = [
            requirement(
                "Section B-B at 1:50",
                r#"{"kind": "view", "id": "bb", "scale": "1:50"}"#,
            ),
            requirement(
                "Clear width 10000 mm",
                r#"{"kind": "dimension", "value": 10, "view": "plan"}"#,
            ),
            requirement(
                "End sill top EL+93.000",
                r#"{"kind": "text", "contains": "el+93.000"}"#,
            ),
            requirement(
                "Three baffle blocks 1000 × 1000",
                r#"{"kind": "outlines", "count": 3, "width": 1, "height": 1, "view": "plan"}"#,
            ),
            requirement("A HATCH layer", r#"{"kind": "layer", "name": "HATCH"}"#),
        ];
        let met = check_document(&sheet(r#", "scale": "1:50""#), &wanted, Some("mm")).unwrap();
        assert!(met.iter().all(|o| o.met), "{met:?}");

        let unmet = [
            requirement(
                "Section B-B at 1:50",
                r#"{"kind": "view", "id": "bb", "scale": "1:50"}"#,
            ),
            requirement(
                "Overall width 11200",
                r#"{"kind": "dimension", "value": 11.2, "view": "plan"}"#,
            ),
            requirement(
                "Apron at EL+92.000",
                r#"{"kind": "text", "contains": "EL+92.000"}"#,
            ),
            requirement(
                "Four baffle blocks",
                r#"{"kind": "outlines", "count": 4, "width": 1, "height": 1, "view": "plan"}"#,
            ),
            requirement("A WATER layer", r#"{"kind": "layer", "name": "WATER"}"#),
            requirement("An elevation", r#"{"kind": "view", "id": "upstream"}"#),
        ];
        let found: Vec<String> = check_document(&sheet(""), &unmet, Some("mm"))
            .unwrap()
            .into_iter()
            .map(|o| {
                assert!(!o.met, "{o:?}");
                o.found
            })
            .collect();
        assert_eq!(
            found,
            vec![
                "view bb is drawn at 1:100",
                "no dimension in view plan reads 11.2; the nearest reads 10",
                "no text says \"EL+92.000\"",
                "3 closed outlines 1 × 1 are drawn in view plan, not 4",
                "there is no layer WATER",
                "there is no view upstream",
            ]
        );
        let finding = finding(&check_document(&sheet(""), &unmet[..1], Some("mm")).unwrap()[0]);
        assert!(
            finding.starts_with(
                "The request requires \"Section B-B at 1:50\", and the drawing does not do it: view bb is drawn at 1:100."
            ),
            "{finding}"
        );
    }

    #[test]
    fn holes_are_outlines_and_need_no_square_drawn_round_them() {
        // A plate's four holes, as the nightly evaluation found a model
        // drawing squares round them to meet a count of outlines.
        let plate = cad_intent::parse(&format!(
            r#"{{"version": {}, "operations": [
               {{"op": "rectangle", "corner": [0, 0], "opposite": [120, 80]}},
               {{"op": "circle", "center": [10, 10], "radius": 5}},
               {{"op": "circle", "center": [110, 10], "radius": 5}},
               {{"op": "circle", "center": [10, 70], "radius": 5}},
               {{"op": "circle", "center": [110, 70], "radius": 5}}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap();
        let wanted = [
            requirement(
                "Four holes of 10 mm diameter",
                r#"{"kind": "outlines", "count": 4, "width": 10, "height": 10}"#,
            ),
            requirement(
                "A 120 × 80 plate",
                r#"{"kind": "outlines", "count": 1, "width": 120, "height": 80}"#,
            ),
        ];
        let met = check_document(&plate, &wanted, None).unwrap();
        assert!(met.iter().all(|o| o.met), "{met:?}");
    }
}
