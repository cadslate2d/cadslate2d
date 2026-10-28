//! The IR read, refused, and compiled (#253): what a model may write and
//! what the engine is handed.

use cad_intent::{
    Answer, COMMANDS, Intent, IntentError, Operation, Pick, Reason, SelectionWord, Step, Targets,
    Unit, VERSION, compile, limits, parse, schema,
};
use proptest::prelude::*;

fn document(operations: &str) -> String {
    format!(r#"{{"version": {VERSION}, "units": "mm", "operations": [{operations}]}}"#)
}

fn answers(steps: &[Step]) -> Vec<String> {
    steps
        .iter()
        .map(|step| match step {
            Step::Select(Targets::Selection(_)) => "select:selection".to_string(),
            Step::Select(Targets::Ids(ids)) => format!("select:{ids:?}"),
            Step::Pick(Pick { id, near }) => format!("pick:{id}:{near:?}"),
            Step::SelectView(id) => format!("view:{id}"),
            Step::Answer { answer, when } => {
                let text = match answer {
                    Answer::Point(p) => format!("{},{}", p.x, p.y),
                    Answer::Length(v) | Answer::Number(v) | Answer::Angle(v) => v.to_string(),
                    Answer::Keyword(k) => (*k).to_string(),
                    Answer::Text(t) => t.clone(),
                    Answer::Enter => String::new(),
                    Answer::InView { view, point } => format!("{view}@{},{}", point.x, point.y),
                    Answer::Staging(view) => format!("staging:{view}"),
                    Answer::Beyond { side, points } => format!("beyond:{side:?}:{}", points.len()),
                };
                match when {
                    Some(prefix) => format!("{text}?{prefix}"),
                    None => text,
                }
            }
        })
        .collect()
}

#[test]
fn a_document_is_read_and_each_operation_compiles_to_its_command_and_answers() {
    let text = document(
        r#"
        {"op": "line", "points": [[0, 0], [100, 0], [100, 50]]},
        {"op": "polyline", "points": [[0, 0], [40, 0], [40, 20]], "closed": true},
        {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]},
        {"op": "circle", "center": [30, 30], "radius": 8},
        {"op": "arc", "start": [0, 0], "through": [10, 10], "end": [20, 0]},
        {"op": "polygon", "sides": 6, "center": [0, 0], "radius": 10},
        {"op": "text", "at": [0, -20], "text": "Plate A", "height": 5},
        {"op": "move", "targets": "selection", "displacement": [0, 25]},
        {"op": "copy", "targets": [12, 15], "displacement": [50, 0]},
        {"op": "rotate", "targets": "selection", "base": [0, 0], "angle": 90},
        {"op": "scale", "targets": [3], "base": [0, 0], "factor": 2},
        {"op": "mirror", "targets": "selection", "axis": [[50, 0], [50, 10]]},
        {"op": "offset", "target": {"id": 7, "near": [50, 0]}, "distance": 10, "toward": [50, 30]},
        {"op": "trim", "edges": [1], "targets": [{"id": 2, "near": [5, 5]}]},
        {"op": "extend", "targets": [{"id": 2}]},
        {"op": "fillet", "radius": 5, "first": {"id": 1}, "second": {"id": 2}},
        {"op": "chamfer", "first_distance": 3, "second_distance": 4, "first": {"id": 1}, "second": {"id": 2}},
        {"op": "array", "targets": "selection", "rows": 2, "columns": 3, "row_spacing": 20, "column_spacing": 30},
        {"op": "polar_array", "targets": [4], "center": [0, 0], "count": 6},
        {"op": "layer", "name": "PLATE", "current": true},
        {"op": "set_properties", "targets": [4], "layer": "PLATE", "color": "red", "lineweight": "0.5"},
        {"op": "dim_linear", "from": [0, 0], "to": [100, 0], "at": [50, -20], "orientation": "horizontal"},
        {"op": "dim_aligned", "from": [0, 0], "to": [30, 40], "at": [10, 30]},
        {"op": "dim_radius", "target": {"id": 9}, "at": [40, 40]},
        {"op": "dim_diameter", "target": {"id": 9}, "at": [40, 40]},
        {"op": "dim_angular", "vertex": [0, 0], "first": [10, 0], "second": [0, 10], "at": [5, 5]},
        {"op": "stair", "height": 2800, "width": 900, "at": [0, 0]},
        {"op": "stair", "height": 3000, "width": 1200, "kind": "helical", "newel": 100, "view": "section", "at": [50, 0], "direction": 0},
        {"op": "iso_line", "points": [[0, 0, 0], [20, 0, 0], [20, 0, 10]]},
        {"op": "iso_polyline", "points": [[0, 0, 0], [0, 10, 0], [0, 10, 10]], "closed": true},
        {"op": "iso_circle", "center": [0, 0, 0], "radius": 10},
        {"op": "elevation", "plan": "PLAN-1"},
        {"op": "elevation", "plan": "PLAN-1", "side": "left", "roof": "gable", "roof_pitch": 35, "at": [0, -12000]},
        {"op": "building_section", "plan": "PLAN-1", "from": [5000, -1000], "to": [5000, 21000]},
        {"op": "part_views", "part": {"steps": [[40, 30], [20, 10]], "bore": 8}, "projection": "third", "section": true},
        {"op": "title_block", "paper": "A2", "scale": 50, "title": "HOUSE", "drawn_by": "LA"}
        "#,
    );
    let intent = parse(&text).unwrap();
    assert_eq!(intent.units, Some(Unit::Mm));
    let compiled = compile(&intent);
    let got: Vec<(&str, Vec<String>)> = compiled
        .iter()
        .map(|c| (c.command, answers(&c.steps)))
        .collect();
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("LINE", vec!["0,0", "100,0", "100,50", ""]),
        ("PLINE", vec!["0,0", "40,0", "40,20", "Close"]),
        ("RECTANG", vec!["0,0", "100,60"]),
        ("CIRCLE", vec!["30,30", "8"]),
        ("ARC", vec!["0,0", "10,10", "20,0"]),
        ("POLYGON", vec!["6", "0,0", "Inscribed in circle", "10"]),
        ("TEXT", vec!["0,-20", "5?Specify height", "", "Plate A"]),
        ("MOVE", vec!["select:selection", "0,0", "0,25"]),
        ("COPY", vec!["select:[12, 15]", "0,0", "50,0", ""]),
        ("ROTATE", vec!["select:selection", "0,0", "90"]),
        ("SCALE", vec!["select:[3]", "0,0", "2"]),
        ("MIRROR", vec!["select:selection", "50,0", "50,10", "No"]),
        (
            "OFFSET",
            vec!["10", "pick:7:Some([50.0, 0.0])", "50,30", ""],
        ),
        (
            "TRIM",
            vec!["select:[1]", "", "pick:2:Some([5.0, 5.0])", ""],
        ),
        ("EXTEND", vec!["", "pick:2:None", ""]),
        ("FILLET", vec!["Radius", "5", "pick:1:None", "pick:2:None"]),
        (
            "CHAMFER",
            vec!["Distance", "3", "4", "pick:1:None", "pick:2:None"],
        ),
        (
            "ARRAY",
            vec![
                "select:selection",
                "Rectangular",
                "2",
                "3",
                "20?Specify the distance between rows",
                "30?Specify the distance between columns",
            ],
        ),
        (
            "ARRAY",
            vec!["select:[4]", "Polar", "0,0", "6", "360", "Yes"],
        ),
        ("LAYER", vec!["New", "PLATE", "Set", "PLATE", ""]),
        (
            "CHPROP",
            vec![
                "select:[4]",
                "LAyer",
                "PLATE",
                "Color",
                "red",
                "LWeight",
                "0.5",
                "",
            ],
        ),
        ("DIMLINEAR", vec!["0,0", "100,0", "Horizontal", "50,-20"]),
        ("DIMALIGNED", vec!["0,0", "30,40", "10,30"]),
        ("DIMRADIUS", vec!["pick:9:None", "40,40"]),
        ("DIMDIAMETER", vec!["pick:9:None", "40,40"]),
        ("DIMANGULAR", vec!["", "0,0", "10,0", "0,10", "5,5"]),
        ("STAIR", vec!["2800", "900", "Straight", "Plan", "0,0", ""]),
        (
            "STAIR",
            vec!["3000", "1200", "Helical", "100", "Section", "50,0", "0"],
        ),
        // 20 along x is 17.32 across and 10 up; 10 up z is straight up.
        (
            "LINE",
            vec!["0,0", "17.32050807568877,10", "17.32050807568877,20", ""],
        ),
        (
            "PLINE",
            vec![
                "0,0",
                "-8.660254037844386,5",
                "-8.660254037844386,15",
                "Close",
            ],
        ),
        // The top plane's isocircle: its long axis level, 10·√(3/2) long,
        // its short one 10/√2.
        (
            "ELLIPSE",
            vec!["Center", "0,0", "12.247448713915889,0", "7.071067811865475"],
        ),
        // The brief's roof, its pitch if it asks, under the plan.
        (
            "ELEVATION",
            vec!["Name", "PLAN-1", "Front", "", "?Specify roof pitch", ""],
        ),
        (
            "ELEVATION",
            vec![
                "Name",
                "PLAN-1",
                "Left",
                "Gable",
                "35?Specify roof pitch",
                "0,-12000",
            ],
        ),
        (
            "BSECTION",
            vec!["Name", "PLAN-1", "5000,-1000", "5000,21000", ""],
        ),
        (
            "PARTVIEWS",
            vec![
                "Json",
                r#"{"steps":[[40.0,30.0],[20.0,10.0]],"bore":8.0}"#,
                "Third",
                "Yes",
                "",
            ],
        ),
        (
            "TITLEBLOCK",
            vec![
                "A2",
                "Landscape",
                "50?Specify the scale",
                "HOUSE",
                "",
                "LA",
                "",
                "",
                "First",
                "",
            ],
        ),
    ];
    assert_eq!(got.len(), expected.len());
    for ((command, steps), (want_command, want_steps)) in got.iter().zip(&expected) {
        assert_eq!(command, want_command);
        assert_eq!(steps, want_steps, "{command}");
    }
    // Every command compiled to is in the allowlist; the allowlist holds no
    // file, host, or settings command.
    for c in &compiled {
        assert!(COMMANDS.contains(&c.command), "{}", c.command);
    }
    for forbidden in [
        "OPEN", "SAVE", "SAVEAS", "DXFOUT", "NEW", "PLOT", "SETTINGS", "WBLOCK", "INSERT",
    ] {
        assert!(!COMMANDS.contains(&forbidden));
    }
}

#[test]
fn what_is_refused_says_where_and_why() {
    let refused = |operations: &str| parse(&document(operations)).unwrap_err();
    let invalid = |operations: &str, field: &str, reason: Reason| match refused(operations) {
        IntentError::Invalid {
            operation: 0,
            field: f,
            reason: r,
            ..
        } => assert_eq!((f, r), (field, reason), "{operations}"),
        other => panic!("{operations}: {other:?}"),
    };
    invalid(
        r#"{"op": "line", "points": [[0, 0]]}"#,
        "points",
        Reason::TooFew(2),
    );
    invalid(
        r#"{"op": "line", "points": [[0, 0], [0, 0]]}"#,
        "points",
        Reason::Told(
            "0 and 1 are both [0, 0]: two points in a row must differ — leave one out".into(),
        ),
    );
    // In a view: where among the view's own operations.
    match refused(
        r#"{"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "line", "points": [[0, 0], [5, 0]]},
            {"op": "polyline", "points": [[0, 0], [5, 5], [5, 5]]}]}"#,
    ) {
        IntentError::Invalid { reason, .. } => assert!(
            reason
                .to_string()
                .ends_with("1 and 2 are both [5, 5]: two points in a row must differ — leave one out (in view `front`, its operation 1, polyline)"),
            "{reason}"
        ),
        other => panic!("{other:?}"),
    }
    invalid(
        r#"{"op": "circle", "center": [0, 0], "radius": 0}"#,
        "radius",
        Reason::NotPositive,
    );
    // Notes drawn inside a view, as a model wrote an urban sheet: what is
    // placed on the sheet goes after the views.
    for inner in [
        r#"{"op": "notes", "lines": ["Schematic."]}"#,
        r#"{"op": "bill_of_materials", "parts": [{"number": 1, "name": "A", "quantity": 1}]}"#,
        r#"{"op": "balloon", "part": 1, "view": "plan", "at": [0, 0], "to": [5, 5]}"#,
        r#"{"op": "title_block", "paper": "A3", "scale": 1, "title": "X", "at": [0, 0]}"#,
    ] {
        match refused(&format!(
            r#"{{"op": "view", "id": "plan", "placed": "principal", "operations": [
                {{"op": "line", "points": [[0, 0], [10, 0]]}}, {inner}]}}"#
        )) {
            IntentError::Invalid {
                field: "operations",
                reason: reason @ Reason::Said(_),
                ..
            } => assert!(reason.to_string().contains("after the views"), "{inner}"),
            other => panic!("{inner}: {other:?}"),
        }
    }
    // A projected view does not look from the front: the principal does.
    match refused(
        r#"{"op": "view", "id": "f2", "placed": "projected", "of": "front", "looking": "from_front",
            "operations": [{"op": "line", "points": [[0, 0], [10, 0]]}]}"#,
    ) {
        IntentError::Invalid {
            field: "looking",
            reason: reason @ Reason::Said(_),
            ..
        } => assert!(reason.to_string().contains("principal view"), "{reason}"),
        other => panic!("{other:?}"),
    }
    // A section given an `of` is told where its parent goes.
    match refused(
        r#"{"op": "view", "id": "aa", "placed": "section", "of": "plan", "cut": {"of": "plan", "from": [0, 0], "to": [10, 0], "toward": [5, 5]},
            "operations": [{"op": "line", "points": [[0, 0], [10, 0]]}]}"#,
    ) {
        IntentError::Invalid {
            field: "of",
            reason: reason @ Reason::Said(_),
            ..
        } => assert!(
            reason.to_string().contains("only for a projected view"),
            "{reason}"
        ),
        other => panic!("{other:?}"),
    }
    // A hatch's region given twice is told which to keep.
    match refused(r#"{"op": "hatch", "inside": [0, 0], "targets": "selection"}"#) {
        IntentError::Invalid {
            field: "inside",
            reason: reason @ Reason::Said(_),
            ..
        } => assert!(reason.to_string().contains("leave `targets` out")),
        other => panic!("{other:?}"),
    }
    invalid(
        r#"{"op": "circle", "center": [1e300, 0], "radius": 1}"#,
        "center",
        Reason::OutOfRange,
    );
    invalid(
        r#"{"op": "polygon", "sides": 2, "center": [0, 0], "radius": 1}"#,
        "sides",
        Reason::TooFew(3),
    );
    invalid(
        r#"{"op": "text", "at": [0, 0], "text": "   "}"#,
        "text",
        Reason::Empty,
    );
    invalid(
        r#"{"op": "move", "targets": [], "displacement": [1, 1]}"#,
        "targets",
        Reason::TooFew(1),
    );
    invalid(r#"{"op": "layer", "name": "A/B"}"#, "name", Reason::BadName);
    // A tangent circle: a radius, or a third object — one of them.
    invalid(
        r#"{"op": "tangent_circle", "first": {"id": 1}, "second": {"id": 2}}"#,
        "radius",
        Reason::Missing,
    );
    invalid(
        r#"{"op": "tangent_circle", "first": {"id": 1}, "second": {"id": 2}, "third": {"id": 3}, "radius": 5}"#,
        "radius",
        Reason::Both("third"),
    );
    invalid(
        r#"{"op": "path_array", "targets": [1], "path": {"id": 2}, "count": 1}"#,
        "count",
        Reason::TooFew(2),
    );
    invalid(
        r#"{"op": "path_array", "targets": [1], "path": {"id": 2}, "count": 3, "spacing": 5}"#,
        "spacing",
        Reason::Both("count"),
    );
    invalid(
        r#"{"op": "grid_array", "targets": [1], "inside": [0, 0], "column_spacing": -5}"#,
        "column_spacing",
        Reason::NotPositive,
    );
    // A constraint: one object or two, as the relation takes; a dimension
    // by points or by an object, as its measure takes.
    invalid(
        r#"{"op": "constrain", "relation": "parallel", "first": {"id": 1}}"#,
        "second",
        Reason::Missing,
    );
    invalid(
        r#"{"op": "constrain", "relation": "horizontal", "first": {"id": 1}, "second": {"id": 2}}"#,
        "second",
        Reason::Unwanted,
    );
    invalid(
        r#"{"op": "constrain_dimension", "measure": "aligned", "at": [0, 5]}"#,
        "points",
        Reason::Missing,
    );
    invalid(
        r#"{"op": "constrain_dimension", "measure": "radius", "points": [[0, 0], [1, 0]], "at": [0, 5]}"#,
        "points",
        Reason::Unwanted,
    );
    invalid(
        r#"{"op": "constrain_dimension", "measure": "angular", "of": {"id": 1}, "at": [0, 5]}"#,
        "and",
        Reason::Missing,
    );
    invalid(
        r#"{"op": "set_parameter", "name": "d1", "value": 0}"#,
        "value",
        Reason::NotPositive,
    );
    // A floor plan's brief: rooms named once, side by side only with rooms
    // of the brief, one of three plans.
    invalid(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 10, "depth": 20}, "rooms": [{"name": "a", "area": 10}, {"name": "a", "area": 5}]}}"#,
        "brief.rooms.name",
        Reason::Repeated,
    );
    invalid(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 10, "depth": 20}, "rooms": [{"name": "a", "area": 10}], "adjacent": [["a", "b"]]}}"#,
        "brief.adjacent",
        Reason::Unknown,
    );
    invalid(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 10, "depth": 20}, "rooms": []}}"#,
        "brief.rooms",
        Reason::TooFew(1),
    );
    invalid(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 10, "depth": 20}, "rooms": [{"name": "a", "area": 10}]}, "alternative": 4}"#,
        "alternative",
        Reason::TooMany(3),
    );
    // Isometric points never twice on one place of the page (#336): one
    // step along x and y is one up z.
    invalid(
        r#"{"op": "iso_line", "points": [[1, 1, 0], [0, 0, 1]]}"#,
        "points",
        Reason::Coincident,
    );
    invalid(
        r#"{"op": "iso_circle", "center": [0, 0, 0], "radius": 0}"#,
        "radius",
        Reason::NotPositive,
    );
    // A stair: sized, and a newel only round which a helix turns (#331).
    invalid(
        r#"{"op": "stair", "height": 0, "width": 900, "at": [0, 0]}"#,
        "height",
        Reason::NotPositive,
    );
    invalid(
        r#"{"op": "stair", "height": 2800, "width": 900, "kind": "u", "newel": 100, "at": [0, 0]}"#,
        "newel",
        Reason::Unwanted,
    );
    invalid(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 10, "depth": 20}, "rooms": [{"name": "a", "area": 10, "stair": true}], "storey_height": -3}}"#,
        "brief.storey_height",
        Reason::NotPositive,
    );
    invalid(
        r#"{"op": "set_properties", "targets": [1]}"#,
        "layer",
        Reason::Nothing,
    );
    invalid(
        r#"{"op": "array", "targets": [1], "rows": 1, "columns": 1, "row_spacing": 1, "column_spacing": 1}"#,
        "rows",
        Reason::Nothing,
    );
    invalid(
        r#"{"op": "rectangle", "corner": [0, 0], "opposite": [0, 5]}"#,
        "opposite",
        Reason::Coincident,
    );
    // NaN is not JSON, so it is refused as JSON; the version and the
    // operation count have errors of their own; an unknown op or field is
    // refused by the schema.
    assert!(matches!(
        refused(r#"{"op": "circle", "center": [0, 0], "radius": NaN}"#),
        IntentError::Json { .. }
    ));
    assert!(matches!(
        parse(r#"{"version": 23, "operations": []}"#).unwrap_err(),
        IntentError::Version {
            found: 23,
            expected: VERSION
        }
    ));
    // A document of an earlier version is read as it was meant (#272).
    assert!(
        parse(r#"{"version": 1, "operations": [{"op": "circle", "center": [0, 0], "radius": 5}]}"#)
            .is_ok()
    );
    // A part is version 2's: its checks hold for it.
    assert_eq!(
        refused(
            r#"{"op": "plate", "corner": [0, 0], "width": 10, "height": 5, "hole_diameter": 2}"#
        )
        .to_string(),
        "operation 0 (plate): hole_offset must be given"
    );
    let many = (0..=limits::OPERATIONS)
        .map(|_| r#"{"op": "circle", "center": [0, 0], "radius": 1}"#)
        .collect::<Vec<_>>()
        .join(",");
    assert!(matches!(
        refused(&many),
        IntentError::TooManyOperations { count, .. } if count == limits::OPERATIONS + 1
    ));
    assert!(matches!(
        refused(r#"{"op": "explode", "targets": [1]}"#),
        IntentError::Json { .. }
    ));
    assert!(matches!(
        refused(r#"{"op": "circle", "center": [0, 0], "radius": 1, "colour": "red"}"#),
        IntentError::Json { .. }
    ));
    assert_eq!(
        refused(r#"{"op": "line", "points": [[0, 0]]}"#).to_string(),
        "operation 0 (line): points needs at least 2"
    );
}

/// The schema is what a model is handed: it changes only with the version.
#[test]
fn the_schema_is_the_recorded_one_or_the_version_is_bumped() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/intent/schema-v22.json"
    );
    let generated = serde_json::to_string_pretty(&schema()).unwrap() + "\n";
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(path, &generated).unwrap();
    }
    let recorded = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        generated, recorded,
        "the schema changed: bump cad_intent::VERSION, re-record with UPDATE_SNAPSHOTS=1, and name the file after the new version"
    );
    assert!(generated.contains(r#""version""#));
    assert_eq!(VERSION, 22);
}

proptest! {
    /// Any valid document survives JSON and is read back the same, and
    /// compiles — to one command of the allowlist per operation.
    #[test]
    fn any_valid_document_round_trips_and_compiles(intent in cad_intent::testing::intent(12)) {
        let text = serde_json::to_string(&intent).unwrap();
        let back = parse(&text).unwrap();
        prop_assert_eq!(&back, &intent);
        let compiled = compile(&back);
        // Every operation compiles to one command, but a view: its own
        // operations, and a run of views one VIEWFRAME (#351).
        let mut wanted = 0;
        let mut in_views = false;
        for operation in &intent.operations {
            // A chain of more than two points is its first dimension and
            // DIMCONTINUE.
            let commands = |op: &cad_intent::Operation| match op {
                cad_intent::Operation::DimChain { points, .. } if points.len() > 2 => 2,
                cad_intent::Operation::Datum { .. } => 0,
                _ => 1,
            };
            match operation {
                cad_intent::Operation::View { operations, .. } => {
                    wanted += operations.iter().map(commands).sum::<usize>() + usize::from(!in_views);
                    in_views = true;
                }
                other => {
                    wanted += commands(other);
                    in_views = false;
                }
            }
        }
        prop_assert_eq!(compiled.len(), wanted);
        let mut last = 0;
        for c in &compiled {
            prop_assert!(c.operation >= last || c.command == "VIEWFRAME");
            last = last.max(c.operation);
            prop_assert!(COMMANDS.contains(&c.command));
            prop_assert!(!c.steps.is_empty());
        }
    }

    /// Any text is read or refused, never a panic.
    #[test]
    fn any_text_is_read_or_refused(text in "\\PC*") {
        let _ = parse(&text);
    }

    /// Any JSON shape, too.
    #[test]
    fn any_json_is_read_or_refused(text in r#"\{"version": [0-9], "operations": \[(\{"op": "[a-z_]{1,12}"(, "[a-z]{1,8}": (-?[0-9]{1,4}|\[-?[0-9]{1,3}, -?[0-9]{1,3}\]|"[a-z]{0,5}"|true|null))*\},?)*\]\}"#) {
        let _ = parse(&text);
    }
}

#[test]
fn the_selection_word_and_an_intent_serialise_as_the_document_says() {
    let intent = Intent {
        version: VERSION,
        units: None,
        operations: vec![Operation::Move {
            targets: Targets::Selection(SelectionWord::Selection),
            displacement: [1.0, 2.0],
        }],
    };
    assert_eq!(
        serde_json::to_string(&intent).unwrap(),
        r#"{"version":22,"operations":[{"op":"move","targets":"selection","displacement":[1.0,2.0]}]}"#
    );
}
