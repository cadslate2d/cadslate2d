//! The intent IR run through the session (#253): each operation of the
//! catalogue makes the drawing a user typing the same would make; what
//! cannot run is refused typed; any valid document runs without a panic.

use cad_core::{EntityId, Point2};
use cad_document::{CircleEntity, Entity, LineEntity};
use cad_intent::{COMMANDS, Intent, VERSION, parse};
use proptest::prelude::*;

use crate::intent::RunError;
use crate::testing::Harness;

fn document(operations: &str) -> Intent {
    parse(&format!(
        r#"{{"version": {VERSION}, "operations": [{operations}]}}"#
    ))
    .unwrap()
}

/// Runs `operations` on a fresh drawing and gives the harness back.
fn run(operations: &str) -> Harness {
    let mut h = Harness::new();
    h.session.run_intent(&document(operations)).unwrap();
    assert_eq!(h.prompt(), "Command:");
    h
}

fn kinds(h: &Harness) -> Vec<&'static str> {
    h.entities()
        .iter()
        .map(|entity| match entity {
            Entity::Line(_) => "line",
            Entity::Circle(_) => "circle",
            Entity::Arc(_) => "arc",
            Entity::Polyline(_) => "polyline",
            Entity::Text(_) => "text",
            Entity::Dimension(_) => "dimension",
            _ => "other",
        })
        .collect()
}

#[test]
fn the_draw_operations_make_what_typing_them_makes() {
    let h = run(r#"
        {"op": "line", "points": [[0, 0], [100, 0], [100, 50]]},
        {"op": "polyline", "points": [[0, 100], [40, 100], [40, 120]], "closed": true},
        {"op": "rectangle", "corner": [200, 0], "opposite": [300, 60]},
        {"op": "circle", "center": [30, 30], "radius": 8},
        {"op": "arc", "start": [0, -50], "through": [10, -40], "end": [20, -50]},
        {"op": "polygon", "sides": 6, "center": [-100, 0], "radius": 10},
        {"op": "text", "at": [0, -100], "text": "Plate A", "height": 5, "rotation": 30}
    "#);
    assert_eq!(
        kinds(&h),
        [
            "line", "line", "polyline", "polyline", "circle", "arc", "polyline", "text"
        ]
    );
    let lines = h.lines();
    assert_eq!(
        lines[0],
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(100.0, 0.0))
    );
    assert_eq!(
        lines[1],
        LineEntity::new(Point2::new(100.0, 0.0), Point2::new(100.0, 50.0))
    );
    let entities = h.entities();
    match &entities[2] {
        Entity::Polyline(p) => assert!(p.closed && p.vertices.len() == 3),
        other => panic!("{other:?}"),
    }
    match &entities[3] {
        Entity::Polyline(p) => assert!(p.closed && p.vertices.len() == 4),
        other => panic!("{other:?}"),
    }
    match &entities[4] {
        Entity::Circle(c) => assert_eq!(*c, CircleEntity::new(Point2::new(30.0, 30.0), 8.0)),
        other => panic!("{other:?}"),
    }
    match &entities[6] {
        Entity::Polyline(p) => assert_eq!(p.vertices.len(), 6),
        other => panic!("{other:?}"),
    }
    match &entities[7] {
        Entity::Text(t) => {
            assert_eq!(t.content, "Plate A");
            assert_eq!(t.height, 5.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_modify_operations_act_on_the_selection_or_on_ids() {
    // A line and a circle; the line is what the first operation made, so
    // "selection" means it; the circle is named by ID.
    let h = run(r#"
        {"op": "circle", "center": [0, 0], "radius": 10},
        {"op": "line", "points": [[0, 50], [100, 50]]},
        {"op": "move", "targets": "selection", "displacement": [0, 10]},
        {"op": "copy", "targets": [1], "displacement": [200, 0]},
        {"op": "rotate", "targets": [2], "base": [0, 60], "angle": 90},
        {"op": "scale", "targets": [3], "base": [200, 0], "factor": 2},
        {"op": "mirror", "targets": [2], "axis": [[0, 0], [0, 1]]}
    "#);
    let entities = h.entities();
    // The circle, the line moved then rotated about its start, the copied
    // circle scaled, and the line's mirror image.
    assert_eq!(kinds(&h), ["circle", "line", "circle", "line"]);
    assert_eq!(
        entities[1],
        Entity::Line(LineEntity::new(
            Point2::new(0.0, 60.0),
            Point2::new(0.0, 160.0)
        ))
    );
    match &entities[2] {
        Entity::Circle(c) => assert_eq!(*c, CircleEntity::new(Point2::new(200.0, 0.0), 20.0)),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        entities[3],
        Entity::Line(LineEntity::new(
            Point2::new(0.0, 60.0),
            Point2::new(0.0, 160.0)
        ))
    );
    assert_eq!(h.session.drawing().len(), 4);
}

#[test]
fn offset_trim_extend_fillet_and_chamfer_pick_the_objects_named() {
    let h = run(r#"
        {"op": "line", "points": [[0, 0], [100, 0]]},
        {"op": "offset", "target": {"id": 1, "near": [50, 0]}, "distance": 10, "toward": [50, 30]},
        {"op": "line", "points": [[50, -20], [50, 40]]},
        {"op": "trim", "edges": [1], "targets": [{"id": 3, "near": [50, -15]}]},
        {"op": "line", "points": [[200, 0], [250, 0]]},
        {"op": "line", "points": [[300, -50], [300, 50]]},
        {"op": "extend", "edges": [5], "targets": [{"id": 4, "near": [245, 0]}]},
        {"op": "line", "points": [[400, 0], [500, 0]]},
        {"op": "line", "points": [[500, 0], [500, 100]]},
        {"op": "fillet", "radius": 10, "first": {"id": 6, "near": [450, 0]}, "second": {"id": 7, "near": [500, 50]}},
        {"op": "line", "points": [[600, 0], [700, 0]]},
        {"op": "line", "points": [[700, 0], [700, 100]]},
        {"op": "chamfer", "first_distance": 5, "second_distance": 5, "first": {"id": 9, "near": [650, 0]}, "second": {"id": 10, "near": [700, 50]}}
    "#);
    let lines = h.lines();
    // The offset copy lies above the line.
    assert!(
        lines
            .iter()
            .any(|l| l.start == Point2::new(0.0, 10.0) && l.end == Point2::new(100.0, 10.0))
    );
    // The vertical line lost its part below the first line.
    assert!(
        lines
            .iter()
            .any(|l| l.start == Point2::new(50.0, 0.0) && l.end == Point2::new(50.0, 40.0))
    );
    // The short line reaches the boundary.
    assert!(
        lines
            .iter()
            .any(|l| l.start == Point2::new(200.0, 0.0) && l.end == Point2::new(300.0, 0.0))
    );
    // The fillet's arc, and the lines shortened to it.
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Arc(a) if (a.radius - 10.0).abs() < 1e-9))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.start == Point2::new(400.0, 0.0) && l.end == Point2::new(490.0, 0.0))
    );
    // The chamfer's bevel.
    assert!(
        lines
            .iter()
            .any(|l| l.start == Point2::new(695.0, 0.0) && l.end == Point2::new(700.0, 5.0))
    );
}

#[test]
fn arrays_layers_properties_and_dimensions() {
    let h = run(r#"
        {"op": "circle", "center": [0, 0], "radius": 5},
        {"op": "array", "targets": "selection", "rows": 2, "columns": 3, "row_spacing": 20, "column_spacing": 30},
        {"op": "circle", "center": [500, 500], "radius": 2},
        {"op": "polar_array", "targets": "selection", "center": [500, 600], "count": 4},
        {"op": "layer", "name": "PLATE A", "current": true},
        {"op": "line", "points": [[0, 200], [100, 200]]},
        {"op": "set_properties", "targets": [1], "layer": "PLATE A", "color": "red"},
        {"op": "dim_linear", "from": [0, 300], "to": [100, 300], "at": [50, 280], "orientation": "horizontal"},
        {"op": "dim_aligned", "from": [0, 400], "to": [30, 440], "at": [10, 430]},
        {"op": "dim_radius", "target": {"id": 1}, "at": [40, 40]},
        {"op": "dim_diameter", "target": {"id": 1}, "at": [-40, -40]},
        {"op": "dim_angular", "vertex": [0, 500], "first": [10, 500], "second": [0, 510], "at": [5, 505]}
    "#);
    let kinds = kinds(&h);
    assert_eq!(
        kinds.iter().filter(|k| **k == "circle").count(),
        1 + 5 + 1 + 3
    );
    assert_eq!(kinds.iter().filter(|k| **k == "dimension").count(), 5);
    let drawing = h.session.drawing();
    let plate = drawing
        .layers()
        .iter()
        .find(|(_, layer)| layer.name == "PLATE A")
        .map(|(id, _)| id)
        .expect("the layer");
    assert_eq!(drawing.current_layer(), plate);
    let properties = h.properties();
    assert_eq!(properties[0].layer, plate);
    assert_ne!(properties[0].color, cad_document::Color::ByLayer);
    // The line drawn after the layer was made current is on it.
    let line = h
        .ids()
        .into_iter()
        .find(|id| matches!(drawing.entity(*id), Some(Entity::Line(_))))
        .unwrap();
    assert_eq!(drawing.properties(line).unwrap().layer, plate);
}

#[test]
fn what_cannot_run_is_refused_typed_and_leaves_the_session_idle() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    let before = h.session.drawing().revision();

    let unknown = document(r#"{"op": "move", "targets": [99], "displacement": [1, 1]}"#);
    assert_eq!(
        h.session.run_intent(&unknown),
        Err(RunError::UnknownEntity {
            operation: 0,
            id: 99
        })
    );
    let nothing = document(r#"{"op": "move", "targets": "selection", "displacement": [1, 1]}"#);
    assert_eq!(
        h.session.run_intent(&nothing),
        Err(RunError::NothingSelected { operation: 0 })
    );
    // A linetype that is not loaded is refused by CHPROP, which asks for
    // it again; the Enter meant to finish goes back to its options, the
    // answers are spent, and the operation is incomplete: the command is
    // cancelled, and the log says what was refused.
    let refused = document(r#"{"op": "set_properties", "targets": [1], "linetype": "Hidden"}"#);
    match h.session.run_intent(&refused) {
        Err(RunError::Refused {
            operation: 0,
            command: "CHPROP",
            reason,
        }) => assert_eq!(
            reason,
            "Linetype \"Hidden\" is not loaded: load it with LINETYPE first."
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(h.prompt(), "Command:");
    assert!(h.log().iter().any(|line| line.contains("is not loaded")));
    assert_eq!(h.session.drawing().revision(), before);
    // An operation that ran says whether the drawing changed.
    let same = document(r#"{"op": "move", "targets": [1], "displacement": [0, 0]}"#);
    let ran = h.session.run_intent(&same).unwrap();
    assert_eq!(ran[0].command, "MOVE");

    // Two lines that never meet: FILLET refuses and asks for the first
    // object again; the operation is refused with FILLET's reason, and the
    // command cancelled.
    h.enter_all(&["L", "0,50", "10,50", ""]);
    let incomplete =
        document(r#"{"op": "fillet", "radius": 10, "first": {"id": 1}, "second": {"id": 2}}"#);
    match h.session.run_intent(&incomplete) {
        Err(RunError::Refused {
            operation: 0,
            command: "FILLET",
            reason,
        }) => assert!(reason.ends_with('.'), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().len(), 2);

    // The first operations run; the report says what ran.
    let partly = document(
        r#"
        {"op": "circle", "center": [0, 0], "radius": 1},
        {"op": "move", "targets": [99], "displacement": [1, 1]}
    "#,
    );
    assert!(matches!(
        h.session.run_intent(&partly),
        Err(RunError::UnknownEntity {
            operation: 1,
            id: 99
        })
    ));
    assert_eq!(
        h.session.drawing().len(),
        3,
        "the circle was made before the stop"
    );

    // A session without the command.
    let mut bare = crate::Session::with_registry(
        cad_document::Drawing::new(),
        crate::CommandRegistry::empty(),
    );
    assert_eq!(
        bare.run_intent(&document(
            r#"{"op": "circle", "center": [0, 0], "radius": 1}"#
        )),
        Err(RunError::NotRegistered {
            operation: 0,
            command: "CIRCLE"
        })
    );
}

#[test]
fn every_command_of_the_catalogue_is_registered_and_the_report_holds_the_log() {
    let h = Harness::new();
    for command in COMMANDS {
        assert!(h.session.is_registered(command), "{command}");
    }
    let mut h = Harness::new();
    let ran = h
        .session
        .run_intent(&document(
            r#"{"op": "circle", "center": [0, 0], "radius": 1}"#,
        ))
        .unwrap();
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0].command, "CIRCLE");
    assert_eq!(ran[0].log[0], "Command: CIRCLE");
}

#[test]
fn the_documents_units_are_converted_to_the_drawings() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    let intent = parse(&format!(
        r#"{{"version": {VERSION}, "units": "m", "operations": [{{"op": "line", "points": [[0, 0], [1.5, 0]]}}]}}"#
    ))
    .unwrap();
    h.session.run_intent(&intent).unwrap();
    assert_eq!(h.lines()[0].end, Point2::new(1500.0, 0.0));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Any valid document runs on a drawing with a few objects without a
    /// panic: every operation runs, or stops the run with a typed refusal,
    /// and the session is idle after.
    #[test]
    fn any_valid_document_runs_or_is_refused_typed(intent in cad_intent::testing::intent(5)) {
        let mut h = Harness::with_entities([
            Entity::Line(LineEntity::new(Point2::new(0.0, 0.0), Point2::new(100.0, 0.0))),
            Entity::Line(LineEntity::new(Point2::new(50.0, -50.0), Point2::new(50.0, 50.0))),
            Entity::Circle(CircleEntity::new(Point2::new(0.0, 0.0), 30.0)),
        ]);
        let ids = h.ids();
        prop_assert!(ids.contains(&EntityId::from_raw(1)));
        let _ = h.session.run_intent(&intent);
        prop_assert_eq!(h.prompt(), "Command:");
    }
}

/// #261: TRIM and EXTEND choose their edges from nothing as they start;
/// the edges an operation names are chosen at their prompt all the same.
#[test]
fn trim_and_extend_take_the_edges_named_not_every_object() {
    let h = run(r#"
        {"op": "line", "points": [[0, 0], [100, 0]]},
        {"op": "line", "points": [[0, 20], [100, 20]]},
        {"op": "line", "points": [[50, -10], [50, 40]]},
        {"op": "trim", "edges": [1], "targets": [{"id": 3, "near": [50, 30]}]}
    "#);
    // Cut at the first line alone: every object an edge would have cut
    // only the piece above the second.
    assert!(
        h.lines()
            .iter()
            .any(|l| l.start == Point2::new(50.0, -10.0) && l.end == Point2::new(50.0, 0.0)),
        "{:?}",
        h.lines()
    );
}

/// #261: a command that goes on after refusing — OFFSET asks for the next
/// object — ends having changed nothing; the operation is refused with
/// the command's words, for the reply to explain.
#[test]
fn an_edit_that_changes_nothing_is_refused_with_the_reason() {
    let mut h = Harness::new();
    h.enter_all(&["C", "0,0", "10"]);
    let before = h.session.drawing().revision();
    let inward =
        document(r#"{"op": "offset", "target": {"id": 1}, "distance": 20, "toward": [0, 0]}"#);
    assert_eq!(
        h.session.run_intent(&inward),
        Err(RunError::Refused {
            operation: 0,
            command: "OFFSET",
            reason: "The offset would leave nothing.".to_owned()
        })
    );
    assert_eq!(h.session.drawing().revision(), before);
    assert_eq!(h.prompt(), "Command:");
}

fn parameters(h: &Harness) -> Vec<String> {
    let mut names: Vec<String> = h
        .session
        .drawing()
        .constraints()
        .iter()
        .filter_map(|(_, c)| c.kind.value().map(|v| format!("{} = {v}", c.name)))
        .collect();
    names.sort();
    names
}

/// #274: a plate made, its parameters in the context the AI is sent, then
/// widened by its width parameter with its holes keeping their places from
/// the edges; a parameter that is not there refused with the reason.
#[test]
fn a_part_is_widened_by_its_parameter_and_its_holes_follow() {
    let mut h = run(
        r#"{"op": "plate", "corner": [0, 0], "width": 120, "height": 80, "hole_diameter": 8, "hole_offset": 10}"#,
    );
    let context = h.session.ai_context();
    assert_eq!(context.objects.len(), 8, "the plate is the selection");
    assert!(
        context
            .parameters
            .iter()
            .any(|p| p.starts_with("d1 = 120: horizontal distance, id ")),
        "{:?}",
        context.parameters
    );
    assert!(
        context
            .parameters
            .iter()
            .any(|p| p.starts_with("dia1 = 8: diameter, of id ")),
        "{:?}",
        context.parameters
    );
    h.session
        .run_intent(&document(
            r#"{"op": "set_parameter", "name": "d1", "value": 140}"#,
        ))
        .unwrap();
    assert!(h.session.drawing().unsatisfied_constraints().is_empty());
    let xs: Vec<f64> = h
        .entities()
        .iter()
        .filter_map(|e| match e {
            Entity::Line(l) => Some([l.start.x, l.end.x]),
            _ => None,
        })
        .flatten()
        .collect();
    let (left, right) = (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!((right - left - 140.0).abs() < 1e-6, "{left} {right}");
    for hole in h.entities().iter().filter_map(|e| match e {
        Entity::Circle(c) => Some(*c),
        _ => None,
    }) {
        let from_edge = (hole.center.x - left).min(right - hole.center.x);
        assert!((from_edge - 10.0).abs() < 1e-6, "{hole:?}");
    }
    assert!(parameters(&h).contains(&"d1 = 140".to_owned()));

    let error = h
        .session
        .run_intent(&document(
            r#"{"op": "set_parameter", "name": "d99", "value": 5}"#,
        ))
        .unwrap_err();
    assert!(
        matches!(&error, RunError::Refused { reason, .. } if reason == "No parameter called \"d99\"."),
        "{error:?}"
    );
}

/// An edit the constraints cannot take is refused with the solver's
/// reason, and the drawing is as it was: six holes a chord of 50 apart
/// do not go round a circle 40 across.
#[test]
fn a_parameter_the_constraints_cannot_take_is_refused_with_the_reason() {
    let mut h = run(
        r#"{"op": "bolt_circle", "center": [0, 0], "pitch_diameter": 100, "count": 6, "hole_diameter": 10}"#,
    );
    let context = h.session.ai_context();
    let pitch = context
        .parameters
        .iter()
        .find(|p| p.contains("= 100: diameter"))
        .and_then(|p| p.split(' ').next())
        .unwrap_or_else(|| panic!("{:?}", context.parameters))
        .to_owned();
    let revision = h.session.drawing().revision();
    let error = h
        .session
        .run_intent(&document(&format!(
            r#"{{"op": "set_parameter", "name": "{pitch}", "value": 40}}"#
        )))
        .unwrap_err();
    match &error {
        RunError::Refused {
            command, reason, ..
        } => {
            assert_eq!(*command, "-PARAMETERS");
            assert!(reason.starts_with("The edit would break"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.session.drawing().revision(), revision);

    // Two lines kept parallel cannot be made perpendicular.
    let mut h = run(r#"{"op": "line", "points": [[0, 0], [100, 0]]},
        {"op": "line", "points": [[0, 20], [100, 20]]},
        {"op": "constrain", "relation": "parallel", "first": {"id": 1}, "second": {"id": 2}}"#);
    let error = h
        .session
        .run_intent(&document(
            r#"{"op": "constrain", "relation": "perpendicular", "first": {"id": 1}, "second": {"id": 2}}"#,
        ))
        .unwrap_err();
    assert!(
        matches!(&error, RunError::Refused { reason, .. } if reason == "It contradicts the other constraints."),
        "{error:?}"
    );
}

#[test]
fn geometric_and_dimensional_constraints_by_intent() {
    // Two lines made parallel: the second turns to the first.
    let h = run(r#"{"op": "line", "points": [[0, 0], [100, 0]]},
        {"op": "line", "points": [[0, 20], [100, 35]]},
        {"op": "constrain", "relation": "parallel", "first": {"id": 1}, "second": {"id": 2}}"#);
    let second = *h
        .session
        .drawing()
        .entity(EntityId::from_raw(2))
        .and_then(Entity::as_line)
        .unwrap();
    assert!((second.end.y - second.start.y).abs() < 1e-7, "{second:?}");
    assert!(h.session.drawing().unsatisfied_constraints().is_empty());

    // A line made horizontal, its length dimensioned and set; a circle's
    // diameter; the angle between two lines.
    let h = run(r#"{"op": "line", "points": [[0, 0], [100, 10]]},
        {"op": "constrain", "relation": "horizontal", "first": {"id": 1}},
        {"op": "constrain_dimension", "measure": "aligned", "of": {"id": 1}, "at": [50, -20], "value": 80},
        {"op": "circle", "center": [200, 0], "radius": 10},
        {"op": "constrain_dimension", "measure": "diameter", "of": {"id": 2}, "at": [220, 20], "value": 30},
        {"op": "line", "points": [[300, 0], [400, 0]]},
        {"op": "line", "points": [[300, 0], [350, 60]]},
        {"op": "constrain", "relation": "coincident", "first": {"id": 3, "near": [300, 0]}, "second": {"id": 4, "near": [300, 0]}},
        {"op": "constrain_dimension", "measure": "angular", "of": {"id": 3, "near": [350, 0]}, "and": {"id": 4, "near": [325, 30]}, "at": [330, 15], "value": 45}"#);
    assert_eq!(
        parameters(&h)[..2],
        ["ang1 = 0.7853981633974483".to_owned(), "d1 = 80".to_owned()]
    );
    let line = *h
        .session
        .drawing()
        .entity(EntityId::from_raw(1))
        .and_then(Entity::as_line)
        .unwrap();
    assert!((line.start.distance_to(line.end) - 80.0).abs() < 1e-6);
    assert!((line.end.y - line.start.y).abs() < 1e-7);
    match h.session.drawing().entity(EntityId::from_raw(2)) {
        Some(Entity::Circle(c)) => assert!((c.radius - 15.0).abs() < 1e-7),
        other => panic!("{other:?}"),
    }
    assert!(h.session.drawing().unsatisfied_constraints().is_empty());
}

/// A dimensional constraint between two points, and a fix at a point.
#[test]
fn constraints_at_points_take_the_point_named() {
    let h = run(r#"{"op": "line", "points": [[0, 0], [100, 0]]},
        {"op": "line", "points": [[100, 0], [100, 50]]},
        {"op": "constrain", "relation": "fix", "first": {"id": 1, "near": [0, 0]}},
        {"op": "constrain_dimension", "measure": "vertical", "points": [[0, 0], [100, 50]], "at": [120, 25], "value": 70}"#);
    let second = *h
        .session
        .drawing()
        .entity(EntityId::from_raw(2))
        .and_then(Entity::as_line)
        .unwrap();
    assert!((second.end.y - 70.0).abs() < 1e-6, "{second:?}");
    let first = *h
        .session
        .drawing()
        .entity(EntityId::from_raw(1))
        .and_then(Entity::as_line)
        .unwrap();
    assert!(first.start.distance_to(Point2::ORIGIN) < 1e-9, "{first:?}");
    assert!(
        first.end.distance_to(Point2::new(100.0, 0.0)) < 1e-9,
        "{first:?}"
    );
    assert_eq!(h.session.drawing().constraints().len(), 2);
}

/// #282: a floor plan by intent — its brief, where it goes, which of the
/// plans — is the drawing FLOORPLAN makes from the same brief; one that
/// cannot be built is refused with the generator's reason.
#[test]
fn a_floor_plan_by_intent_is_the_one_floorplan_draws() {
    let brief = r#"{"terrain": {"width": 15, "depth": 20}, "rooms": [{"name": "living", "area": 24, "entrance": true}, {"name": "kitchen", "area": 10}, {"name": "bedroom", "area": 12}, {"name": "bath", "area": 4.5}], "adjacent": [["living", "kitchen"]]}"#;
    let by_intent = run(&format!(
        r#"{{"op": "floor_plan", "brief": {brief}, "at": [10, 5], "alternative": 2}}"#
    ));
    let mut by_command = Harness::new();
    by_command.enter_all(&["FLOORPLAN", "B", brief, "10,5", "N", ""]);
    assert!(
        by_command.log().last().unwrap().starts_with("Plan 2 drawn"),
        "{:?}",
        by_command.log()
    );
    assert_eq!(by_intent.entities(), by_command.entities());
    assert!(by_intent.entities().len() > 20);
    // The first plan is another drawing.
    let first = run(&format!(
        r#"{{"op": "floor_plan", "brief": {brief}, "at": [10, 5]}}"#
    ));
    assert_ne!(first.entities(), by_intent.entities());

    let mut h = Harness::new();
    let error = h
        .session
        .run_intent(&document(
            r#"{"op": "floor_plan", "brief": {"terrain": {"width": 8, "depth": 8}, "rooms": [{"name": "living", "area": 40}, {"name": "bedroom", "area": 30}]}}"#,
        ))
        .unwrap_err();
    assert!(
        matches!(&error, RunError::Refused { command: "FLOORPLAN", reason, .. }
            if reason.starts_with("This cannot be built: The rooms need at least 66.5 m²")),
        "{error:?}"
    );
    assert!(h.entities().is_empty());
}
