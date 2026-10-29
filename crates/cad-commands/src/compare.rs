//! What a revision changed (#358): two documents run, each on an empty
//! drawing, and their objects compared view by view and layer by layer.
//! "Correct only the stairs, don't touch the roof" is then checked rather
//! than trusted: a revision that redraws more than it was asked to is told
//! so, and the user is shown what changed and what did not.
//!
//! An object is the same when it is on the same layer of the same view with
//! the same geometry, measured from where the view was placed — a view the
//! sheet moved, because another grew, is not a view that changed.

use std::collections::BTreeMap;

use cad_core::Point2;
use cad_document::Drawing;
use cad_geometry::Transform;
use cad_intent::Intent;

use crate::compose::{section_group, sheet_of, view_group, views_on};

/// Where an object is that is not in a view: the sheet itself, or a
/// drawing that is no sheet.
pub const SHEET: &str = "sheet";

/// What changed on one layer of one view: how many objects went, how many
/// came. An object moved or reshaped is one of each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub view: String,
    pub layer: String,
    pub removed: usize,
    pub added: usize,
}

impl Change {
    /// `view/layer`, as a revision names what it changes.
    pub fn key(&self) -> String {
        format!("{}/{}", self.view, self.layer)
    }
}

/// Each object of `drawing` as compared: its view, its layer, and its
/// geometry from the view's placement, as text to the thousandth.
fn objects(drawing: &Drawing) -> BTreeMap<(String, String, String), usize> {
    // Each object's view and the view's origin on the sheet.
    let mut placed: BTreeMap<cad_core::EntityId, (String, Point2)> = BTreeMap::new();
    if let Some(sheet) = sheet_of(drawing) {
        let views = views_on(drawing, &sheet.group);
        let members = |name: &str| -> Vec<cad_core::EntityId> {
            drawing
                .groups()
                .iter()
                .find(|(_, g)| g.name == name)
                .map(|(_, g)| g.members.clone())
                .unwrap_or_default()
        };
        for view in &views {
            for id in members(&view_group(&sheet.group, &view.id)) {
                placed.insert(id, (view.id.clone(), view.origin));
            }
            // A section's mark is drawn on the view it cuts, and moves with it.
            if let Some((letter, of)) = &view.section
                && let Some(parent) = views.iter().find(|v| &v.id == of)
            {
                for id in members(&section_group(&sheet.group, *letter)) {
                    placed.insert(id, (parent.id.clone(), parent.origin));
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    for (id, entity) in drawing.entities() {
        let (view, origin) = placed
            .get(&id)
            .cloned()
            .unwrap_or_else(|| (SHEET.to_owned(), Point2::ORIGIN));
        let layer = drawing
            .properties(id)
            .and_then(|p| drawing.layers().get(p.layer))
            .map_or_else(String::new, |layer| layer.name.clone());
        let local = entity.transformed(&Transform::translation(Point2::ORIGIN - origin));
        *out.entry((view, layer, rounded(&format!("{local:?}"))))
            .or_insert(0) += 1;
    }
    out
}

/// Every number with a fraction in `text` to the thousandth: two runs of
/// the same geometry placed apart agree to far better than that.
fn rounded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i].is_ascii_digit()
            || (chars[i] == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit));
        let inside = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if starts && !inside {
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_ascii_digit()
                    || chars[j] == '.'
                    || chars[j] == 'e'
                    || chars[j] == '-')
            {
                if chars[j] == '-' && chars[j - 1] != 'e' {
                    break;
                }
                j += 1;
            }
            let token: String = chars[i..j].iter().collect();
            match token.parse::<f64>() {
                Ok(value) if token.contains('.') || token.contains('e') => {
                    let value = if value.abs() < 5e-4 { 0.0 } else { value };
                    out.push_str(&format!("{value:.3}"));
                }
                _ => out.push_str(&token),
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `intent` run on an empty drawing in `unit`, as the review runs it.
fn drawn(intent: &Intent, unit: Option<&str>) -> Option<crate::Session> {
    let mut session = crate::Session::new();
    session.resize(800.0, 600.0);
    if let Some(unit) = unit {
        session.submit_text("UNITS");
        session.submit_text(unit);
    }
    session.run_intent(intent).ok()?;
    Some(session)
}

/// What `after` changed of `before`, view by view and layer by layer, in
/// the order of the views' names; empty when it changed nothing. `None`
/// when either does not run on an empty drawing.
pub fn compare(before: &Intent, after: &Intent, unit: Option<&str>) -> Option<Vec<Change>> {
    let was = objects(drawn(before, unit)?.drawing());
    let now = objects(drawn(after, unit)?.drawing());
    let mut changes: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
    for (key, count) in &was {
        let kept = now.get(key).copied().unwrap_or(0);
        if *count > kept {
            changes.entry((key.0.clone(), key.1.clone())).or_default().0 += count - kept;
        }
    }
    for (key, count) in &now {
        let had = was.get(key).copied().unwrap_or(0);
        if *count > had {
            changes.entry((key.0.clone(), key.1.clone())).or_default().1 += count - had;
        }
    }
    Some(
        changes
            .into_iter()
            .map(|((view, layer), (removed, added))| Change {
                view,
                layer,
                removed,
                added,
            })
            .collect(),
    )
}

/// The changes no entry of `declared` covers: an entry `view/layer`
/// covers that layer of that view, `view` the whole view, `*/layer` that
/// layer in every view.
pub fn undeclared<'a>(changes: &'a [Change], declared: &[String]) -> Vec<&'a Change> {
    changes
        .iter()
        .filter(|change| {
            !declared.iter().any(|d| {
                let d = d.trim();
                d == change.view
                    || d == change.key()
                    || d.strip_prefix("*/") == Some(change.layer.as_str())
            })
        })
        .collect()
}

/// The changes as a person reads them: `B-B · STAIR: 10 replaced`.
pub fn described(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .map(|c| {
            let what = match (c.removed, c.added) {
                (0, n) => format!("{n} added"),
                (n, 0) => format!("{n} removed"),
                (r, a) if r == a => format!("{r} replaced"),
                (r, a) => format!("{r} removed, {a} added"),
            };
            let layer = if c.layer.is_empty() { "0" } else { &c.layer };
            format!("{} · {layer}: {what}", c.view)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(stair: &str, roof: &str, extra: &str) -> Intent {
        cad_intent::parse(&format!(
            r#"{{"version": {}, "operations": [
               {{"op": "title_block", "paper": "A2", "portrait": true, "scale": 50, "title": "X", "at": [0, 0]}},
               {{"op": "view", "id": "AA", "placed": "principal", "operations": [
                   {{"op": "layer", "name": "WALL", "current": true}},
                   {{"op": "rectangle", "corner": [0, 0], "opposite": [7000, 6000]}},
                   {{"op": "layer", "name": "ROOF", "current": true}},
                   {{"op": "line", "points": [[-400, 6000], [3500, {roof}], [7400, 6000]]}}{extra}]}},
               {{"op": "view", "id": "BB", "placed": "free", "operations": [
                   {{"op": "layer", "name": "STAIR", "current": true}},
                   {{"op": "line", "points": [[0, 0], [{stair}, 3200]]}}]}}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap()
    }

    #[test]
    fn the_same_document_changes_nothing() {
        let a = sheet("4000", "7050", "");
        assert_eq!(compare(&a, &a, Some("mm")), Some(vec![]));
    }

    #[test]
    fn a_stair_redrawn_is_its_view_and_layer_only_though_the_sheet_moved() {
        let before = sheet("4000", "7050", "");
        // The stair is longer: B-B grows, the sheet is laid out again.
        let after = sheet("5300", "7050", "");
        let changes = compare(&before, &after, Some("mm")).unwrap();
        assert_eq!(
            changes,
            vec![Change {
                view: "BB".into(),
                layer: "STAIR".into(),
                removed: 1,
                added: 1
            }]
        );
        assert_eq!(described(&changes), vec!["BB · STAIR: 1 replaced"]);
        assert!(undeclared(&changes, &["BB/STAIR".into()]).is_empty());
        assert!(undeclared(&changes, &["BB".into()]).is_empty());
        assert!(undeclared(&changes, &["*/STAIR".into()]).is_empty());
        assert_eq!(undeclared(&changes, &["AA/ROOF".into()]).len(), 1);
    }

    #[test]
    fn a_roof_lost_while_the_stair_was_fixed_is_seen() {
        let before = sheet("4000", "7050", "");
        let after = sheet("5300", "6800", "");
        let changes = compare(&before, &after, Some("mm")).unwrap();
        let keys: Vec<String> = changes.iter().map(Change::key).collect();
        assert_eq!(keys, vec!["AA/ROOF", "BB/STAIR"]);
        let left = undeclared(&changes, &["BB/STAIR".into()]);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].key(), "AA/ROOF");
    }

    #[test]
    fn numbers_are_compared_to_the_thousandth() {
        assert_eq!(
            rounded("Line { start: Point2 { x: 1.00000000001, y: -0.0000001 }, id: 12 }"),
            "Line { start: Point2 { x: 1.000, y: 0.000 }, id: 12 }"
        );
    }
}
