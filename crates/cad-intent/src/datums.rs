//! Datums (ADR-0041 §3, #361): named values of the object a composed sheet
//! shows, read into the coordinates that name them before the document is
//! typed.
//!
//! Inside a view, every point `[x, y]` is the view's own (u, v). Which axis
//! of the object each is, and its sign, follows the view's orientation:
//!
//! | looking     | u   | v   |
//! | ----------- | --- | --- |
//! | from_front  | +W  | +H  |
//! | from_behind | −W  | +H  |
//! | from_left   | −D  | +H  |
//! | from_right  | +D  | +H  |
//! | from_above  | +W  | +D  |
//! | from_below  | +W  | −D  |
//!
//! So `{"datum": "reactor"}` at width +200 is u = +200 in the front view
//! and u = −200 from behind. A datum on an axis the view does not show, an
//! unknown one, one outside a view, and one declared for a view that does
//! not use it are refused.

use serde_json::{Value, json};

use crate::validate::{IntentError, Reason};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    Width,
    Depth,
    Height,
}

impl Axis {
    fn name(self) -> &'static str {
        match self {
            Self::Width => "width",
            Self::Depth => "depth",
            Self::Height => "height",
        }
    }
}

/// The view a datum is read in, for what a refusal says: its id and where
/// it looks from.
#[derive(Clone, Copy)]
struct Seen<'a> {
    id: &'a str,
    looking: &'a str,
}

/// For a view's orientation, the axis and sign of u and of v.
fn axes(looking: &str) -> Option<[(Axis, f64); 2]> {
    use Axis::{Depth, Height, Width};
    Some(match looking {
        "from_front" => [(Width, 1.0), (Height, 1.0)],
        "from_behind" => [(Width, -1.0), (Height, 1.0)],
        "from_left" => [(Depth, -1.0), (Height, 1.0)],
        "from_right" => [(Depth, 1.0), (Height, 1.0)],
        "from_above" => [(Width, 1.0), (Depth, 1.0)],
        "from_below" => [(Width, 1.0), (Depth, -1.0)],
        _ => return None,
    })
}

fn axis(name: &str) -> Option<Axis> {
    match name {
        "width" => Some(Axis::Width),
        "depth" => Some(Axis::Depth),
        "height" => Some(Axis::Height),
        _ => None,
    }
}

struct Datum {
    name: String,
    axis: Axis,
    value: f64,
}

fn refuse(operation: usize, op: &'static str, field: &'static str, reason: Reason) -> IntentError {
    IntentError::Invalid {
        operation,
        op,
        field,
        reason,
    }
}

fn names_a_datum(value: &Value) -> Option<&str> {
    value
        .as_object()
        .filter(|o| o.len() == 1)?
        .get("datum")?
        .as_str()
}

/// Whether `value` holds a datum reference anywhere.
fn holds_datum(value: &Value) -> bool {
    match value {
        Value::Object(o) => o.contains_key("datum") || o.values().any(holds_datum),
        Value::Array(items) => items.iter().any(holds_datum),
        _ => false,
    }
}

/// Replaces each datum named in the points of `value` by its number, as
/// the view oriented by `orientation` reads it; the datums it used.
fn resolve_in(
    value: &mut Value,
    orientation: Option<[(Axis, f64); 2]>,
    datums: &[Datum],
    used: &mut Vec<String>,
    at: (usize, &'static str),
    seen: Seen<'_>,
) -> Result<(), IntentError> {
    match value {
        Value::Array(items)
            if items.len() == 2 && items.iter().any(|v| names_a_datum(v).is_some()) =>
        {
            for (i, item) in items.iter_mut().enumerate() {
                let Some(name) = names_a_datum(item) else {
                    continue;
                };
                let Some(datum) = datums.iter().find(|d| d.name == name) else {
                    return Err(refuse(
                        at.0,
                        at.1,
                        "datum",
                        Reason::Told(format!(
                            "`{name}` in view `{}` names no datum: declare it with a `datum` operation before the views, or write the number",
                            seen.id
                        )),
                    ));
                };
                let Some(axes) = orientation else {
                    // A view that does not say where it looks from has no
                    // axes for a datum.
                    return Err(refuse(
                        at.0,
                        at.1,
                        "datum",
                        Reason::Told(format!(
                            "`{name}` is used in view `{}`, which does not say where it looks from: give view `{}` a `looking` — a plan `from_above`; a section or elevation whose x runs along the principal view's x `from_front`, one whose x runs along the plan's y `from_left` or `from_right` — or write the number",
                            seen.id, seen.id
                        )),
                    ));
                };
                let (axis, sign) = axes[i];
                if axis != datum.axis {
                    let coordinate = ["x", "y"][i];
                    return Err(refuse(
                        at.0,
                        at.1,
                        "datum",
                        Reason::Told(format!(
                            "`{name}` is a {} datum, but it is the {coordinate} of a point in view `{}`, which looks {}: there x is {} and y is {}. Use it only as the coordinate along its own axis, or write the number. The principal view always looks from the front; a plan is a view looking `from_above`",
                            datum.axis.name(),
                            seen.id,
                            seen.looking,
                            axes[0].0.name(),
                            axes[1].0.name(),
                        )),
                    ));
                }
                used.push(datum.name.clone());
                *item = json!(sign * datum.value);
            }
            Ok(())
        }
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|item| resolve_in(item, orientation, datums, used, at, seen)),
        Value::Object(o) => {
            if o.contains_key("datum") {
                // A datum stands for a coordinate of a point, nothing else.
                return Err(refuse(
                    at.0,
                    at.1,
                    "datum",
                    Reason::Said(
                        "stands only for a coordinate of a point `[x, y]`, not a radius, a length, or a count",
                    ),
                ));
            }
            o.values_mut()
                .try_for_each(|item| resolve_in(item, orientation, datums, used, at, seen))
        }
        _ => Ok(()),
    }
}

/// Reads every datum of the document into the coordinates that name it.
pub(crate) fn resolve(document: &mut Value) -> Result<(), IntentError> {
    resolve_with(document, &mut Vec::new())
}

fn resolve_with(document: &mut Value, used_in: &mut Used) -> Result<(), IntentError> {
    let Some(operations) = document.get_mut("operations").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let mut datums: Vec<Datum> = Vec::new();
    for (index, op) in operations.iter().enumerate() {
        if op["op"] != "datum" {
            continue;
        }
        let (Some(name), Some(axis), Some(value)) = (
            op["name"].as_str(),
            op["axis"].as_str().and_then(axis),
            op["value"].as_f64(),
        ) else {
            continue; // the typed reading refuses it
        };
        if datums.iter().any(|d| d.name == name) {
            return Err(refuse(index, "datum", "name", Reason::Repeated));
        }
        datums.push(Datum {
            name: name.to_owned(),
            axis,
            value,
        });
    }

    // Every view that uses a datum and does not say where it looks from,
    // named at once, so one correction puts them all right (#358).
    let unlooked: Vec<(usize, String)> = operations
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            op["op"] == "view"
                && op["placed"] != "principal"
                && op["looking"].as_str().is_none()
                && holds_datum(&op["operations"])
        })
        .map(|(index, op)| {
            (
                index,
                format!("`{}`", op["id"].as_str().unwrap_or_default()),
            )
        })
        .collect();
    if let Some((first, _)) = unlooked.first() {
        let ids: Vec<&str> = unlooked.iter().map(|(_, id)| id.as_str()).collect();
        return Err(refuse(
            *first,
            "view",
            "looking",
            Reason::Told(format!(
                "is missing from {}, which use datums: give each a `looking` — a plan `from_above`; a section or elevation whose x runs along the principal view's x `from_front`, one whose x runs along the plan's y `from_left` or `from_right` — or write the numbers",
                ids.join(", ")
            )),
        ));
    }
    for (index, op) in operations.iter_mut().enumerate() {
        if op["op"] == "view" {
            let id = op["id"].as_str().unwrap_or_default().to_owned();
            let looking = match op["placed"].as_str() {
                Some("principal") => "from_front".to_owned(),
                _ => op["looking"].as_str().unwrap_or_default().to_owned(),
            };
            let orientation = axes(&looking);
            let mut used = Vec::new();
            if let Some(inner) = op.get_mut("operations") {
                let seen = Seen {
                    id: &id,
                    looking: &looking,
                };
                resolve_in(
                    inner,
                    orientation,
                    &datums,
                    &mut used,
                    (index, "view"),
                    seen,
                )?;
            }
            used_in.push((id, used));
        } else if op["op"] != "datum" && holds_datum(op) {
            // Outside a view there are no axes to read a datum along.
            return Err(refuse(
                index,
                "view",
                "datum",
                Reason::Said("is used outside a view, where there are no axes to read it along"),
            ));
        }
    }
    Ok(())
}

/// Where each datum of a document is drawn, and the views it is declared
/// for and is not drawn in (ADR-0041 §7): a finding of the sheet's review,
/// not a refusal, since the model may yet put it right. Empty for a
/// document that is not one.
pub fn unused_datums(text: &str) -> Vec<String> {
    let Ok(mut value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    datum_use(&mut value)
        .map(|(declared, used_in)| {
            let mut findings = Vec::new();
            for (name, views) in declared {
                for view in views {
                    let used = used_in
                        .iter()
                        .any(|(id, names)| *id == view && names.contains(&name));
                    if !used {
                        findings.push(format!(
                            "Datum {name} is declared for view {view} but not drawn there: draw its feature at {{\"datum\": \"{name}\"}} in that view, or leave the view out of its `views`."
                        ));
                    }
                }
            }
            findings
        })
        .unwrap_or_default()
}

type Declared = Vec<(String, Vec<String>)>;
type Used = Vec<(String, Vec<String>)>;

/// The datums declared, with their `views`, and the datums each view uses.
fn datum_use(document: &mut Value) -> Result<(Declared, Used), IntentError> {
    let declared: Declared = document["operations"]
        .as_array()
        .map(|ops| {
            ops.iter()
                .filter(|op| op["op"] == "datum")
                .filter_map(|op| {
                    Some((
                        op["name"].as_str()?.to_owned(),
                        op["views"]
                            .as_array()
                            .map(|v| {
                                v.iter()
                                    .filter_map(|n| n.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut used: Used = Vec::new();
    resolve_with(document, &mut used)?;
    Ok((declared, used))
}

/// The schema's points widened so that either coordinate may name a datum
/// (`{"datum": name}`), which the model is told it may do inside a view.
#[cfg(feature = "schema")]
pub(crate) fn widen_points(schema: &mut Value) {
    fn walk(value: &mut Value) {
        match value {
            Value::Object(o) => {
                let is_point = o.get("type") == Some(&json!("array"))
                    && o.get("minItems") == Some(&json!(2))
                    && o.get("maxItems") == Some(&json!(2))
                    && o.get("items") == Some(&json!({"format": "double", "type": "number"}));
                if is_point {
                    o.insert(
                        "items".to_owned(),
                        json!({"anyOf": [
                            {"format": "double", "type": "number"},
                            {"$ref": "#/$defs/DatumRef"}
                        ]}),
                    );
                    return;
                }
                o.values_mut().for_each(walk);
            }
            Value::Array(items) => items.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(schema);
    if let Some(defs) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        defs.insert(
            "DatumRef".to_owned(),
            json!({
                "description": "A coordinate given as a datum declared by a `datum` operation: inside a view, its value read along the axis this coordinate is in the view's orientation, sign included.",
                "type": "object",
                "properties": {"datum": {"type": "string"}},
                "required": ["datum"],
                "additionalProperties": false
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::{IntentError, Operation, VERSION, parse};

    fn document(operations: &str) -> Result<crate::Intent, IntentError> {
        parse(&format!(
            r#"{{"version": {VERSION}, "operations": [{operations}]}}"#
        ))
    }

    fn rectangle_in(intent: &crate::Intent, view: &str) -> ([f64; 2], [f64; 2]) {
        intent
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::View { id, operations, .. } if id == view => {
                    operations.iter().find_map(|inner| match inner {
                        Operation::Rectangle { corner, opposite } => Some((*corner, *opposite)),
                        _ => None,
                    })
                }
                _ => None,
            })
            .unwrap()
    }

    /// The orientation table: a feature at W = +200 is at u = +200 from the
    /// front and −200 from behind; a depth signed from the left and right;
    /// a height the same everywhere.
    #[test]
    fn a_datum_is_read_with_the_sign_each_orientation_gives() {
        let intent = document(
            r#"
            {"op": "datum", "name": "reactor", "axis": "width", "value": 200},
            {"op": "datum", "name": "chest", "axis": "depth", "value": 170},
            {"op": "datum", "name": "shoulder", "axis": "height", "value": 1450, "views": ["front", "rear", "left", "right"]},
            {"op": "view", "id": "front", "placed": "principal", "operations": [
                {"op": "rectangle", "corner": [{"datum": "reactor"}, 0], "opposite": [300, {"datum": "shoulder"}]}]},
            {"op": "view", "id": "rear", "placed": "projected", "of": "front", "looking": "from_behind", "operations": [
                {"op": "rectangle", "corner": [{"datum": "reactor"}, 0], "opposite": [-300, {"datum": "shoulder"}]}]},
            {"op": "view", "id": "left", "placed": "projected", "of": "front", "looking": "from_left", "operations": [
                {"op": "rectangle", "corner": [{"datum": "chest"}, 0], "opposite": [0, {"datum": "shoulder"}]}]},
            {"op": "view", "id": "right", "placed": "projected", "of": "front", "looking": "from_right", "operations": [
                {"op": "rectangle", "corner": [{"datum": "chest"}, 0], "opposite": [0, {"datum": "shoulder"}]}]},
            {"op": "view", "id": "top", "placed": "projected", "of": "front", "looking": "from_above", "operations": [
                {"op": "rectangle", "corner": [{"datum": "reactor"}, {"datum": "chest"}], "opposite": [0, 0]}]},
            {"op": "view", "id": "bottom", "placed": "projected", "of": "front", "looking": "from_below", "operations": [
                {"op": "rectangle", "corner": [{"datum": "reactor"}, {"datum": "chest"}], "opposite": [0, 0]}]}
        "#,
        )
        .unwrap();
        assert_eq!(
            rectangle_in(&intent, "front"),
            ([200.0, 0.0], [300.0, 1450.0])
        );
        assert_eq!(rectangle_in(&intent, "rear").0, [-200.0, 0.0]);
        assert_eq!(rectangle_in(&intent, "left").0, [-170.0, 0.0]);
        assert_eq!(rectangle_in(&intent, "right").0, [170.0, 0.0]);
        assert_eq!(rectangle_in(&intent, "top").0, [200.0, 170.0]);
        assert_eq!(rectangle_in(&intent, "bottom").0, [200.0, -170.0]);
        // The same height everywhere it is drawn.
        for view in ["front", "rear", "left", "right"] {
            assert_eq!(rectangle_in(&intent, view).1[1], 1450.0, "{view}");
        }
    }

    #[test]
    fn a_datum_where_it_cannot_be_read_is_refused() {
        let refused = |text: &str| document(text).unwrap_err().to_string();
        let shoulder = r#"{"op": "datum", "name": "shoulder", "axis": "height", "value": 1450}"#;
        // On an axis the view does not show: a height across. Said with
        // the datum, the view, and its axes, so it can be put right (#358).
        let across = refused(&format!(
            r#"{shoulder}, {{"op": "view", "id": "front", "placed": "principal", "operations": [
            {{"op": "line", "points": [[{{"datum": "shoulder"}}, 0], [1, 1]]}}]}}"#
        ));
        assert!(
            across.contains("`shoulder` is a height datum, but it is the x of a point in view `front`, which looks from_front: there x is width and y is height"),
            "{across}"
        );
        // Unknown.
        let unknown = refused(
            r#"{"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "line", "points": [[{"datum": "nope"}, 0], [1, 1]]}]}"#,
        );
        assert!(
            unknown.contains("`nope` in view `front` names no datum"),
            "{unknown}"
        );
        // Outside a view.
        assert!(
            refused(&format!(
                r#"{shoulder}, {{"op": "line", "points": [[0, {{"datum": "shoulder"}}], [1, 1]]}}"#
            ))
            .contains("datum")
        );
        // In free views that do not say where they look from: all named
        // at once, so one correction puts them right.
        let unlooked = refused(&format!(
            r#"{shoulder}, {{"op": "view", "id": "x", "placed": "free", "operations": [
            {{"op": "line", "points": [[0, {{"datum": "shoulder"}}], [1, 1]]}}]}},
            {{"op": "view", "id": "y", "placed": "free", "operations": [
            {{"op": "line", "points": [[0, {{"datum": "shoulder"}}], [1, 1]]}}]}}"#
        ));
        assert!(
            unlooked.contains("looking is missing from `x`, `y`, which use datums"),
            "{unlooked}"
        );
        // A section along the length looks from the front (version 22):
        // its y is the height.
        let along = document(&format!(
            r#"{shoulder}, {{"op": "view", "id": "aa", "placed": "free", "looking": "from_front", "operations": [
            {{"op": "line", "points": [[0, {{"datum": "shoulder"}}], [1, 1]]}}]}}"#
        ));
        assert!(along.is_ok(), "{along:?}");
        // For a number that is not a coordinate.
        assert!(
            refused(&format!(
                r#"{shoulder}, {{"op": "view", "id": "front", "placed": "principal", "operations": [
            {{"op": "circle", "center": [0, 0], "radius": {{"datum": "shoulder"}}}}]}}"#
            ))
            .contains("datum")
        );
        // Declared for a view that does not use it: read, and said by the
        // sheet's review rather than refused.
        let unused = r#"{"version": 21, "operations": [{"op": "datum", "name": "shoulder", "axis": "height", "value": 1450, "views": ["front", "side"]},
            {"op": "view", "id": "front", "placed": "principal", "operations": [
                {"op": "line", "points": [[0, {"datum": "shoulder"}], [1, 1]]}]}]}"#;
        assert!(crate::parse(unused).is_ok());
        let findings = crate::unused_datums(unused);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("Datum shoulder is declared for view side"));
        // Twice.
        assert!(refused(&format!("{shoulder}, {shoulder}")).contains("name"));
    }
}
