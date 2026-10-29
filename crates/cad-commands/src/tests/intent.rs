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

/// #325: `erase` takes the selection or named objects away; a plan drawn
/// is a group whose description holds its brief, which the context the
/// AI is sent carries, so the plan is drawn again revised — erased, then
/// planned anew with windows, at the same place.
#[test]
fn erase_removes_objects_and_a_drawn_plan_is_revised_from_its_groups_brief() {
    let mut h = run(r#"
        {"op": "circle", "center": [0, 0], "radius": 10},
        {"op": "line", "points": [[0, 50], [100, 50]]},
        {"op": "erase", "targets": "selection"},
        {"op": "circle", "center": [50, 0], "radius": 5},
        {"op": "erase", "targets": [1]}
    "#);
    assert_eq!(kinds(&h), ["circle"]);
    let brief = r#"{"terrain": {"width": 12, "depth": 20}, "setbacks": {"front": 5},
        "rooms": [{"name": "living room", "area": 25, "entrance": true}, {"name": "kitchen", "area": 10},
                  {"name": "bedroom 1", "area": 12}, {"name": "bedroom 2", "area": 10}, {"name": "bathroom", "area": 5}],
        "adjacent": [["kitchen", "living room"]]}"#;
    h.session
        .run_intent(&document(&format!(
            r#"{{"op": "floor_plan", "brief": {brief}, "at": [100, 100]}}"#
        )))
        .unwrap();
    let drawn = h.session.drawing().len();
    assert!(drawn > 50);
    // Selected whole, the plan's group and its brief go with the request.
    let all: Vec<cad_core::EntityId> = h.session.drawing().entities().map(|(id, _)| id).collect();
    h.session.select_only(&all);
    let context = h.session.ai_context();
    assert_eq!(context.groups.len(), 1);
    let group = &context.groups[0];
    assert_eq!(group.name, "PLAN-1");
    assert_eq!(
        group.members,
        drawn - 1,
        "the plan's objects; the circle is not in it"
    );
    assert!(
        group.description.starts_with(r#"{"brief":{"#),
        "{}",
        group.description
    );
    assert!(
        group.description.contains(r#""at":[100,100]"#)
            && group.description.contains(r#""alternative":1"#)
    );
    // Revised as the AI would: erased, then planned again with windows.
    let revised = brief.trim_end_matches('}').to_owned() + r#", "windows": 1.5}"#;
    h.session
        .run_intent(&document(&format!(
            r#"{{"op": "erase", "targets": "selection"}}, {{"op": "floor_plan", "brief": {revised}, "at": [100, 100]}}"#
        )))
        .unwrap();
    let windows = h
        .session
        .drawing()
        .layers()
        .find("WINDOW")
        .expect("windows drawn");
    let drawing = h.session.drawing();
    let glass = drawing
        .entities()
        .filter(|(id, _)| drawing.properties(*id).is_some_and(|p| p.layer == windows))
        .count();
    assert!(glass >= 6, "{glass}");
    // The old plan's group went with its objects, so the name is free again.
    let groups: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, g)| g.name.clone())
        .collect();
    assert_eq!(groups, ["PLAN-1"]);
    assert!(
        drawing
            .groups()
            .iter()
            .all(|(_, g)| g.description.contains(r#""windows":1.5"#))
    );
    assert!(!kinds(&h).contains(&"circle"), "the selection was erased");
}

/// #327: `dim_scale` sets the current style's scale, so the dimension that
/// follows is legible at a drawing 1200 units across: text 10, value 1200.
#[test]
fn dim_scale_draws_the_dimensions_that_follow_legibly() {
    let h = run(r#"
        {"op": "dim_scale", "scale": 4},
        {"op": "line", "points": [[0, 0], [1200, 0]]},
        {"op": "dim_linear", "from": [0, 0], "to": [1200, 0], "at": [600, -60]}
    "#);
    let drawing = h.session.drawing();
    assert_eq!(
        drawing
            .tables()
            .dim_style(drawing.current_dim_style())
            .scale,
        4.0
    );
    let dimension = drawing
        .entities()
        .find_map(|(_, e)| match e {
            Entity::Dimension(d) => Some(d),
            _ => None,
        })
        .unwrap();
    let layout = dimension.layout(drawing.tables().dim_style(dimension.style));
    assert_eq!(layout.text.height, 10.0);
    assert!((layout.value - 1200.0).abs() < 1e-9);
}

/// #329: `members` makes a truss's centrelines into members, chords 10
/// wide and webs 5, as the command does; the centrelines go.
#[test]
fn members_make_a_truss_of_its_centrelines() {
    let h = run(r#"
        {"op": "line", "points": [[0, 0], [1200, 0]]},
        {"op": "line", "points": [[0, 0], [600, 200]]},
        {"op": "line", "points": [[600, 200], [1200, 0]]},
        {"op": "line", "points": [[150, 50], [300, 0]]},
        {"op": "line", "points": [[300, 0], [450, 150]]},
        {"op": "line", "points": [[450, 150], [600, 0]]},
        {"op": "line", "points": [[600, 0], [750, 150]]},
        {"op": "line", "points": [[750, 150], [900, 0]]},
        {"op": "line", "points": [[900, 0], [1050, 50]]},
        {"op": "members", "groups": [
            {"targets": [1, 2, 3], "width": 10},
            {"targets": [4, 5, 6, 7, 8, 9], "width": 5}
        ]}
    "#);
    assert_eq!(kinds(&h), ["polyline"; 9]);
    assert!(
        h.entities()
            .iter()
            .all(|e| matches!(e, Entity::Polyline(p) if p.closed))
    );
}

/// #336: an isometric box's edges and a hole in its top, by points on the
/// isometric axes; the hole is the top plane's isocircle, as ELLIPSE
/// Isocircle draws it.
#[test]
fn isometric_operations_draw_on_the_axes() {
    let h = run(r#"
        {"op": "iso_polyline", "points": [[0, 0, 0], [40, 0, 0], [40, 30, 0], [0, 30, 0]], "closed": true},
        {"op": "iso_line", "points": [[40, 0, 0], [40, 0, 20]]},
        {"op": "iso_circle", "center": [20, 15, 20], "radius": 8}
    "#);
    assert_eq!(kinds(&h), ["polyline", "line", "other"]);
    let edge = h.lines()[0];
    let corner = cad_core::iso::map(40.0, 0.0, 0.0);
    assert!(edge.start.distance_to(corner) < 1e-9);
    assert!((edge.end - edge.start - cad_core::Vec2::new(0.0, 20.0)).length() < 1e-9);
    let Entity::Ellipse(hole) = h.entities()[2].clone() else {
        panic!()
    };
    let mut typed = Harness::new();
    typed.enter_all(&["SNAP", "S", "I", "10", "ELLIPSE", "I", "0,0", "8"]);
    let Entity::Ellipse(by_hand) = typed.entities()[0].clone() else {
        panic!()
    };
    assert!(
        hole.center
            .distance_to(cad_core::iso::map(20.0, 15.0, 20.0))
            < 1e-9
    );
    assert!((hole.major - by_hand.major).length() < 1e-9);
    assert!((hole.ratio - by_hand.ratio).abs() < 1e-9);
}

/// #337: a plan and its façade in one document — the plan's group named
/// as the first free — the same drawing as typing them; the brief's
/// building gives the roof when the elevation does not.
#[test]
fn an_elevation_of_the_plan_just_drawn_is_the_one_typed() {
    let brief = r#"{"terrain": {"width": 15, "depth": 20}, "rooms": [{"name": "living", "area": 24, "entrance": true}, {"name": "kitchen", "area": 10}], "windows": 1.2, "building": {"roof": "hip", "roof_pitch": 25}}"#;
    let by_intent = run(&format!(
        r#"{{"op": "floor_plan", "brief": {brief}}}, {{"op": "elevation", "plan": "PLAN-1", "side": "right", "at": [30, 0]}}"#
    ));
    let mut by_hand = Harness::new();
    by_hand.enter_all(&["FLOORPLAN", "B", brief, "", ""]);
    by_hand.enter_all(&["ELEVATION", "N", "PLAN-1", "R", "", "", "30,0"]);
    assert!(
        by_hand
            .log()
            .iter()
            .any(|l| l.starts_with("Right elevation of PLAN-1")),
        "{:?}",
        by_hand.log()
    );
    assert_eq!(by_intent.entities(), by_hand.entities());
    // The hip roof of the brief, its pitch the brief's.
    let drawing = by_intent.session.drawing();
    let roof = drawing.layers().find("ELEV-ROOF").expect("a roof drawn");
    assert!(
        drawing
            .entities()
            .any(|(id, _)| drawing.properties(id).is_some_and(|p| p.layer == roof))
    );
}

/// #338: a section through a plan drawn by the same document is the one
/// typed.
#[test]
fn a_building_section_is_the_one_typed() {
    let brief = r#"{"terrain": {"width": 15, "depth": 20}, "rooms": [{"name": "living", "area": 24, "entrance": true}, {"name": "kitchen", "area": 10}], "windows": 1.2}"#;
    let by_intent = run(&format!(
        r#"{{"op": "floor_plan", "brief": {brief}}}, {{"op": "building_section", "plan": "PLAN-1", "from": [0, 2], "to": [15, 2], "at": [0, -12]}}"#
    ));
    let mut by_hand = Harness::new();
    by_hand.enter_all(&["FLOORPLAN", "B", brief, "", ""]);
    by_hand.enter_all(&["BSECTION", "N", "PLAN-1", "0,2", "15,2", "0,-12"]);
    assert!(
        by_hand
            .log()
            .iter()
            .any(|l| l.starts_with("Section A-A of PLAN-1")),
        "{:?}",
        by_hand.log()
    );
    assert_eq!(by_intent.entities(), by_hand.entities());
}

/// #339: a part's views by intent are the ones typed.
#[test]
fn part_views_are_the_ones_typed() {
    let part = r#"{"profile": [[0, 0], [100, 0], [100, 20], [30, 20], [30, 60], [0, 60]], "depth": 40, "holes": [{"axis": "z", "at": [70, 20], "diameter": 10}]}"#;
    let by_intent = run(&format!(
        r#"{{"op": "part_views", "part": {part}, "section": true, "at": [10, 10]}}"#
    ));
    let mut by_hand = Harness::new();
    by_hand.enter_all(&["PARTVIEWS", "J", part, "F", "Y", "10,10"]);
    assert!(
        by_hand
            .log()
            .iter()
            .any(|l| l.starts_with("Part views drawn as PART-1")),
        "{:?}",
        by_hand.log()
    );
    assert_eq!(by_intent.entities(), by_hand.entities());
}

/// #340: a title block by intent is the one typed.
#[test]
fn a_title_block_is_the_one_typed() {
    let by_intent = run(
        r#"{"op": "title_block", "paper": "A3", "scale": 20, "title": "BRACKET", "number": "M-7", "projection": "third", "at": [-50, -50]}"#,
    );
    let mut by_hand = Harness::new();
    by_hand.enter_all(&[
        "TITLEBLOCK",
        "A3",
        "L",
        "20",
        "BRACKET",
        "M-7",
        "",
        "",
        "",
        "T",
        "-50,-50",
    ]);
    assert!(
        by_hand
            .log()
            .last()
            .unwrap()
            .starts_with("A3 landscape sheet drawn as TITLE-1"),
        "{:?}",
        by_hand.log()
    );
    assert_eq!(by_intent.entities(), by_hand.entities());
}

/// #331: `stair` draws what STAIR typed draws, in plan and in section; a
/// brief's stair room has its stair drawn in it, for its storey.
#[test]
fn a_stair_is_the_one_typed_and_a_briefs_stair_room_holds_one() {
    for (operation, typed) in [
        (
            r#"{"op": "stair", "height": 2.8, "width": 0.9, "kind": "u", "at": [1, 2]}"#,
            vec!["STAIR", "2.8", "0.9", "U", "Plan", "1,2", ""],
        ),
        (
            r#"{"op": "stair", "height": 3, "width": 1.2, "kind": "helical", "newel": 0.1, "view": "section", "at": [0, 0], "direction": 0}"#,
            vec!["STAIR", "3", "1.2", "Helical", "0.1", "Section", "0,0", "0"],
        ),
    ] {
        let by_intent = run(operation);
        let mut by_hand = Harness::new();
        by_hand.enter_all(&typed);
        assert!(!by_intent.entities().is_empty());
        assert_eq!(by_intent.entities(), by_hand.entities(), "{operation}");
    }
    let h = run(
        r#"{"op": "floor_plan", "brief": {"terrain": {"width": 15, "depth": 20}, "rooms": [
            {"name": "living", "area": 24, "entrance": true},
            {"name": "kitchen", "area": 10},
            {"name": "hall", "area": 12, "stair": true}
        ], "adjacent": [["living", "kitchen"], ["living", "hall"]], "storey_height": 3}}"#,
    );
    let texts: Vec<String> = h
        .entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Text(t) => Some(t.content),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"18 R × 16.7 / T 29.7".to_owned()),
        "{texts:?}"
    );
    assert!(h.session.drawing().layers().find("STAIR").is_some());
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

/// A spline passes through every point it is given, in order (#345): the
/// ogee of a spillway's crest, drawn through its surveyed points.
#[test]
fn a_spline_passes_through_its_points() {
    let h = run(
        r#"{"op": "spline", "points": [[0, 110], [2, 109.5], [5, 107.5], [8, 104.5], [12, 100], [16, 96], [20, 92]]}"#,
    );
    let entities = h.entities();
    let [Entity::Spline(spline)] = entities.as_slice() else {
        panic!("{entities:?}");
    };
    let wanted = [
        (0.0, 110.0),
        (2.0, 109.5),
        (5.0, 107.5),
        (8.0, 104.5),
        (12.0, 100.0),
        (16.0, 96.0),
        (20.0, 92.0),
    ];
    assert_eq!(spline.fit, wanted.map(|(x, y)| Point2::new(x, y)).to_vec());
    // The curve itself goes through each, at the parameter it was fitted
    // at: the spline interpolates, it does not approximate.
    let curve = spline.spline();
    let parameters = cad_geometry::spline::fit_parameters(&spline.fit);
    for (at, t) in spline.fit.iter().zip(parameters) {
        let on = curve.point_at(t);
        assert!(on.distance_to(*at) < 1e-9, "{at:?}: the curve is at {on:?}");
    }
}

/// A hatch fills the loop around a point, or the loops of its targets,
/// with the pattern, angle, and scale asked for (#345); a pattern the
/// table does not have is refused, and nothing is drawn.
#[test]
fn a_hatch_fills_its_boundary_with_its_pattern() {
    let h = run(r#"
        {"op": "rectangle", "corner": [0, 0], "opposite": [100, 50]},
        {"op": "hatch", "inside": [50, 25], "pattern": "ANSI31", "angle": 15, "scale": 2}
    "#);
    let entities = h.entities();
    let Entity::Hatch(hatch) = &entities[1] else {
        panic!("{entities:?}");
    };
    assert_eq!(hatch.loops.len(), 1);
    match &hatch.style {
        cad_document::HatchStyle::Pattern {
            name, angle, scale, ..
        } => {
            assert_eq!(name, "ANSI31");
            assert!((angle - 15f64.to_radians()).abs() < 1e-12);
            assert_eq!(*scale, 2.0);
        }
        other => panic!("{other:?}"),
    }

    // What was just drawn, as the targets: solid by default.
    let h = run(r#"
        {"op": "polyline", "points": [[0, 0], [40, 0], [40, 30], [0, 30]], "closed": true},
        {"op": "hatch", "targets": "selection"}
    "#);
    let entities = h.entities();
    let Entity::Hatch(hatch) = &entities[1] else {
        panic!("{entities:?}");
    };
    assert_eq!(hatch.style, cad_document::HatchStyle::Solid);

    let mut h = Harness::new();
    let refused = document(
        r#"
        {"op": "rectangle", "corner": [0, 0], "opposite": [100, 50]},
        {"op": "hatch", "inside": [50, 25], "pattern": "AR-CONC"}
    "#,
    );
    match h.session.run_intent(&refused) {
        Err(RunError::Refused {
            operation: 1,
            command: "HATCH",
            reason,
        }) => assert!(reason.contains("AR-CONC"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(h.prompt(), "Command:");
}

/// A composed sheet by intent (#351, ADR-0041): each view's operations
/// drawn in its own coordinates, then placed together — the same sheet as
/// typing them and VIEWFRAME makes.
#[test]
fn views_of_a_sheet_are_drawn_in_their_own_coordinates_and_placed_together() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "BLOCK", "projection": "third", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]}]},
        {"op": "view", "id": "left", "title": "LEFT SIDE", "placed": "projected", "of": "front", "looking": "from_left", "operations": [
            {"op": "rectangle", "corner": [-40, 0], "opposite": [0, 60]}]},
        {"op": "view", "id": "top", "placed": "projected", "of": "front", "looking": "from_above", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 40]}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let sheet = crate::compose::sheet_of(h.session.drawing()).unwrap();
    let views = crate::compose::views_on(h.session.drawing(), &sheet.group);
    let at = |id: &str| views.iter().find(|v| v.id == id).unwrap().origin;
    assert_eq!(views.len(), 3);
    assert_eq!(at("left").y, at("front").y);
    assert_eq!(at("top").x, at("front").x);
    assert!(at("left").x < at("front").x && at("top").y > at("front").y);

    // Typed: the same.
    let mut t = Harness::new();
    t.enter_all(&[
        "UNITS",
        "mm",
        "TITLEBLOCK",
        "A3",
        "Landscape",
        "1",
        "BLOCK",
        "",
        "",
        "",
        "",
        "Third",
        "0,0",
    ]);
    t.enter_all(&["RECTANG", "0,0", "100,60"]);
    let front = *t.ids().last().unwrap();
    t.enter_all(&["RECTANG", "-40,0", "0,60"]);
    let left = *t.ids().last().unwrap();
    t.enter_all(&["RECTANG", "0,0", "100,40"]);
    let top = *t.ids().last().unwrap();
    t.session.select_only(&[front]);
    t.enter_all(&["VIEWFRAME", "0,0", "front", "", "Principal", "Yes"]);
    t.session.select_only(&[left]);
    t.enter_all(&[
        "",
        "0,0",
        "left",
        "LEFT SIDE",
        "PRojected",
        "front",
        "Left",
        "Yes",
    ]);
    t.session.select_only(&[top]);
    t.enter_all(&["", "0,0", "top", "", "PRojected", "front", "Above", "No"]);
    let typed = crate::compose::views_on(t.session.drawing(), "TITLE-1");
    for view in &views {
        let same = typed.iter().find(|v| v.id == view.id).unwrap();
        assert_eq!(same.origin, view.origin, "{}", view.id);
    }
}

/// A detail at its own scale by intent (#352): drawn true size, enlarged
/// by the program, its dimension reading true.
#[test]
fn a_detail_view_by_intent_is_drawn_at_its_ratio() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 3, "title": "SHAFT", "projection": "third", "at": [0, 0]},
        {"op": "view", "id": "detail", "title": "DETAIL A", "placed": "free", "ratio": 2, "operations": [
            {"op": "line", "points": [[0, 0], [10, 0]]},
            {"op": "dim_linear", "from": [0, 0], "to": [10, 0], "at": [5, -5]}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    let members = drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == "VIEW-detail")
        .map(|(_, g)| g.members.clone())
        .unwrap();
    let line = members.iter().find_map(|id| match drawing.entity(*id) {
        Some(Entity::Line(l)) => Some(l.start.distance_to(l.end)),
        _ => None,
    });
    assert!((line.unwrap() - 60.0).abs() < 1e-9);
    let shown = drawing.entities().find_map(|(_, e)| match e {
        Entity::Dimension(d) => Some(d.layout(drawing.tables().dim_style(d.style)).text.content),
        _ => None,
    });
    assert!(shown.unwrap().starts_with("10.00"));
}

/// A section by intent (#353): its cut in the front view's own
/// coordinates, marked there once both are placed, the section titled.
#[test]
fn a_section_view_by_intent_is_marked_across_its_parent() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "BLOCK", "projection": "third", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]}]},
        {"op": "view", "id": "aa", "placed": "section", "cut": {"of": "front", "from": [50, -10], "to": [50, 70], "toward": [100, 30]}, "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [60, 60]},
            {"op": "hatch", "inside": [30, 30], "pattern": "ANSI31"}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    assert!(drawing.groups().iter().any(|(_, g)| g.name == "SECTION-A"));
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Text(t) if t.content == "SECTION A-A"))
    );
}

/// A bill of materials and a balloon by intent (#354): the balloon's
/// leader starts where the front view put the point on the part, given in
/// the view's own coordinates; a part not in the bill is refused.
#[test]
fn a_bill_of_materials_and_a_balloon_by_intent() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "BLOCK", "projection": "third", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]}]},
        {"op": "bill_of_materials", "parts": [{"number": 1, "name": "Block", "quantity": 1, "material": "Steel"}]},
        {"op": "balloon", "part": 1, "view": "front", "at": [100, 60], "to": [130, 80]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let front = crate::compose::views_on(h.session.drawing(), "TITLE-1")
        .into_iter()
        .find(|v| v.id == "front")
        .unwrap();
    let drawing = h.session.drawing();
    let leader = drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == "BALLOON-1-1")
        .and_then(|(_, g)| drawing.entity(g.members[0]))
        .unwrap();
    let Entity::Line(leader) = leader else {
        panic!("{leader:?}")
    };
    assert_eq!(
        leader.start,
        front.origin + cad_core::Vec2::new(100.0, 60.0)
    );

    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    let refused = h.session.run_intent(&document(
        r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "BLOCK", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]}]},
        {"op": "bill_of_materials", "parts": [{"number": 1, "name": "Block", "quantity": 1}]},
        {"op": "balloon", "part": 2, "view": "front", "at": [0, 0], "to": [10, 10]}
    "#,
    ));
    assert!(
        matches!(
            refused,
            Err(RunError::Refused {
                command: "BALLOON",
                ..
            })
        ),
        "{refused:?}"
    );
}

/// A chain of dimensions by intent (#355): DIMLINEAR then DIMCONTINUE, the
/// dimension line beyond the view's geometry on the side asked; a second
/// chain on that side beyond the first. And notes, placed as a view.
#[test]
fn a_dimension_chain_stands_beyond_the_geometry_and_the_chains_before() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "PLATE", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]},
            {"op": "dim_chain", "points": [[0, 0], [40, 0], [100, 0]], "side": "below"},
            {"op": "dim_chain", "points": [[0, 0], [100, 0]], "side": "below"}]},
        {"op": "notes", "lines": ["All dimensions in mm."], "width": 60}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    let mut dims: Vec<&cad_document::DimensionEntity> = drawing
        .entities()
        .filter_map(|(_, e)| match e {
            Entity::Dimension(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(dims.len(), 3);
    let mut values: Vec<f64> = dims.iter().map(|d| d.value()).collect();
    values.sort_by(f64::total_cmp);
    assert_eq!(values, [40.0, 60.0, 100.0]);
    let front = crate::compose::views_on(drawing, "TITLE-1")
        .into_iter()
        .find(|v| v.id == "front")
        .unwrap();
    // The chain's two dimensions on one line, 10 below the plate; the
    // overall one further out.
    dims.sort_by(|a, b| a.value().total_cmp(&b.value()));
    let below = |d: &cad_document::DimensionEntity| front.origin.y - d.line_point.y;
    assert!((below(dims[0]) - below(dims[1])).abs() < 1e-9);
    assert!(below(dims[0]) >= 10.0 - 1e-9);
    assert!(
        below(dims[2]) > below(dims[0]) + 5.0,
        "{} {}",
        below(dims[2]),
        below(dims[0])
    );
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::MText(m) if m.content == "All dimensions in mm."))
    );
}

/// A chain measures along the axis its points spread along, said to
/// DIMLINEAR rather than left to where its line falls (#358): a point at
/// the same place along it as the one before makes no dimension, and the
/// chain draws the rest; a first dimension short beside the rest stays
/// horizontal; a horizontal chain said to the right still measures across.
#[test]
fn a_chain_point_that_measures_nothing_is_left_out() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A1", "scale": 50, "title": "PLATE", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]},
            {"op": "dim_chain", "points": [[0, 0], [40, 0], [40, 60], [100, 0]], "side": "below"}]},
        {"op": "view", "id": "road", "placed": "free", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [20000, 300]},
            {"op": "dim_chain", "points": [[0, 0], [3000, 0], [17000, 0], [20000, 0]], "side": "below"}]},
        {"op": "view", "id": "reactor", "placed": "free", "operations": [
            {"op": "circle", "center": [0, 0], "radius": 65},
            {"op": "dim_chain", "points": [[-65, 0], [65, 0]], "side": "right"}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let mut values: Vec<f64> = h
        .entities()
        .iter()
        .filter_map(|e| match e {
            Entity::Dimension(d) => Some(d.value()),
            _ => None,
        })
        .collect();
    values.sort_by(f64::total_cmp);
    // And a road's chain, its first dimension short beside a long one: as
    // an urban section's was, horizontal throughout.
    // A diameter's chain said to the right still measures across it. To a
    // hundredth: the sheet is moved to the middle of its paper after.
    let wanted = [40.0, 60.0, 130.0, 3000.0, 3000.0, 14000.0];
    assert_eq!(values.len(), wanted.len(), "{values:?}");
    for (value, wanted) in values.iter().zip(wanted) {
        assert!((value - wanted).abs() < 0.01, "{values:?}");
    }
}

/// An operation that makes nothing — a layer — leaves the selection as it
/// was (#358): a centre line drawn, its layer made, then the line moved onto
/// it by "the selection", as a pump sheet's side view did.
#[test]
fn a_layer_made_between_keeps_what_the_line_before_it_made_selected() {
    let h = run(r#"{"op": "line", "points": [[0, 0], [100, 0]]},
           {"op": "layer", "name": "CENTER", "linetype": "CENTER"},
           {"op": "set_properties", "targets": "selection", "layer": "CENTER"}"#);
    let drawing = h.session.drawing();
    let center = drawing.layers().find("CENTER").unwrap();
    assert!(
        drawing
            .entities()
            .all(|(id, _)| drawing.record(id).unwrap().properties.layer == center)
    );
}

/// A layer's lineweight by intent (version 23, #358): outlines at 0.5 mm,
/// set as LAYER's LWeight does; and the sheet's own layers drawn with the
/// weights their lines take — section marks heavier than a table's rules.
#[test]
fn a_layer_is_given_its_lineweight_and_the_sheet_layers_theirs() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"{"op": "layer", "name": "OUTLINE", "lineweight": "0.5", "current": true},
               {"op": "title_block", "paper": "A3", "scale": 1, "title": "X", "at": [0, 0]},
               {"op": "view", "id": "front", "placed": "principal", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [60, 40]}]},
               {"op": "view", "id": "aa", "placed": "section", "cut": {"of": "front", "from": [30, -5], "to": [30, 45], "toward": [40, 20]},
                "operations": [{"op": "rectangle", "corner": [0, 0], "opposite": [20, 40]}]},
               {"op": "notes", "lines": ["Schematic."]}"#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    let weight = |name: &str| {
        drawing
            .layers()
            .get(drawing.layers().find(name).unwrap())
            .unwrap()
            .lineweight
    };
    use cad_document::Lineweight::Hundredths;
    assert_eq!(weight("OUTLINE"), Hundredths(50));
    assert_eq!(weight("SECTION-MARKS"), Hundredths(50));
    assert_eq!(weight("VIEW-TITLES"), Hundredths(35));
    assert_eq!(weight("NOTES"), Hundredths(25));
}

/// A view is staged clear of the sheet (#358): an A0 sheet at 1:1000 is
/// over a million millimetres wide, and its border and title block reached
/// into a view staged a million units away, cutting the hatch of a parcel
/// there into a strip.
#[test]
fn a_view_on_a_large_sheet_is_drawn_clear_of_the_sheet_itself() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"{"op": "title_block", "paper": "A0", "scale": 1000, "title": "U01", "at": [0, 0]},
               {"op": "view", "id": "plan", "placed": "free", "looking": "from_above", "operations": [
                   {"op": "rectangle", "corner": [0, 0], "opposite": [190000, 140000]},
                   {"op": "segments", "segments": [[[38000, 0], [38000, 140000]], [[152000, 0], [152000, 140000]]]},
                   {"op": "hatch", "inside": [171000, 20000], "pattern": "ANSI31", "scale": 1000}]}"#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let hatch = h
        .entities()
        .into_iter()
        .find_map(|e| match e {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .unwrap();
    assert!(
        (hatch.area() - 38000.0 * 140000.0).abs() < 1.0,
        "{}",
        hatch.area()
    );
}

/// Separate segments (version 23, #358): a row of parcel boundaries drawn
/// as pairs, each a line of its own — no line from one boundary's top to
/// the next one's foot, which a `line` through the same points draws.
#[test]
fn segments_are_lines_joined_to_nothing() {
    let h = run(
        r#"{"op": "segments", "segments": [[[38, 0], [38, 140]], [[76, 0], [76, 140]], [[114, 0], [114, 140]]]}"#,
    );
    let lines: Vec<(Point2, Point2)> = h
        .entities()
        .iter()
        .filter_map(|e| match e {
            Entity::Line(l) => Some((l.start, l.end)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines.iter().all(|(a, b)| a.x == b.x),
        "all vertical: {lines:?}"
    );
    // The same points as one `line`: a chain, its diagonals among it.
    let chain = run(r#"{"op": "line", "points": [[38, 0], [38, 140], [76, 0], [76, 140]]}"#);
    assert_eq!(chain.entities().len(), 3);
}

/// A layer with a colour and a linetype by intent (#356): CENTER, loaded
/// from the program's own, and gold, the nearest index colour.
#[test]
fn a_layer_is_made_with_a_colour_and_a_linetype_loaded_for_it() {
    let h = run(
        r#"{"op": "layer", "name": "CENTRE-LINES", "color": "gold", "linetype": "CENTER", "current": true}"#,
    );
    let drawing = h.session.drawing();
    let layer = drawing
        .layers()
        .get(drawing.layers().find("CENTRE-LINES").unwrap())
        .unwrap();
    assert_eq!(layer.color, cad_document::Color::parse("gold").unwrap());
    let cad_document::Linetype::Named(id) = layer.linetype else {
        panic!("{:?}", layer.linetype)
    };
    assert_eq!(
        drawing.linetypes().get(id).unwrap().name.to_uppercase(),
        "CENTER"
    );
    assert_eq!(
        drawing.current_layer(),
        drawing.layers().find("CENTRE-LINES").unwrap()
    );
}

/// Datums across views (#361, ADR-0041 §3): the shoulder drawn at one
/// datum in the front and the side view is at one height on the sheet.
#[test]
fn a_feature_drawn_at_a_datum_in_two_views_agrees_on_the_sheet() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A1", "scale": 5, "title": "ARMOUR", "projection": "third", "at": [0, 0]},
        {"op": "datum", "name": "shoulder", "axis": "height", "value": 1450, "views": ["front", "side"]},
        {"op": "datum", "name": "chest", "axis": "depth", "value": 340},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [560, 1950]},
            {"op": "line", "points": [[-100, {"datum": "shoulder"}], [660, {"datum": "shoulder"}]]}]},
        {"op": "view", "id": "side", "placed": "projected", "of": "front", "looking": "from_left", "operations": [
            {"op": "rectangle", "corner": [{"datum": "chest"}, 0], "opposite": [0, 1950]},
            {"op": "line", "points": [[-400, {"datum": "shoulder"}], [60, {"datum": "shoulder"}]]}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    let shoulder_in = |view: &str| {
        let members = drawing
            .groups()
            .iter()
            .find(|(_, g)| g.name == format!("VIEW-{view}"))
            .map(|(_, g)| g.members.clone())
            .unwrap();
        members
            .iter()
            .find_map(|id| match drawing.entity(*id) {
                Some(Entity::Line(l)) => Some(l.start.y),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(shoulder_in("front"), shoulder_in("side"));
}

/// Each view is drawn in a place of its own before it is placed (#358):
/// two views' rectangles at the same local coordinates, each hatched from
/// a point inside, each hatch filling its own — not meeting the other's.
#[test]
fn each_view_is_drawn_clear_of_the_others_before_it_is_placed() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "title_block", "paper": "A3", "scale": 1, "title": "SECTIONS", "at": [0, 0]},
        {"op": "view", "id": "front", "placed": "principal", "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [100, 60]},
            {"op": "rectangle", "corner": [20, 20], "opposite": [40, 40]}]},
        {"op": "view", "id": "aa", "placed": "section", "cut": {"of": "front", "from": [50, -10], "to": [50, 70], "toward": [100, 30]}, "operations": [
            {"op": "rectangle", "corner": [0, 0], "opposite": [60, 60]},
            {"op": "hatch", "inside": [30, 30], "pattern": "ANSI31"}]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let drawing = h.session.drawing();
    let hatch = drawing
        .entities()
        .find_map(|(_, e)| match e {
            Entity::Hatch(hatch) => Some(hatch.clone()),
            _ => None,
        })
        .unwrap();
    // One loop, the section's own 60 × 60: not the front's hole.
    assert_eq!(hatch.loops.len(), 1);
    let b = Entity::Hatch(hatch).bounds(drawing.tables());
    assert!((b.max().x - b.min().x - 60.0).abs() < 1e-6);
}

/// A second sheet in a drawing that has one (#358): the maintainer's
/// house, its ground floor accepted, then its upper floor — the same view
/// ids, a section, notes, and its title block named at the origin as the
/// first's was. It is drawn to the right of the first, said in the log;
/// its views, section mark, and notes are its own, inside its border; and
/// nothing of the first sheet moves.
#[test]
fn a_second_sheet_is_drawn_beside_the_first_with_views_named_as_its_own() {
    let floor = |title: &str| {
        document(&format!(
            r#"
        {{"op": "title_block", "paper": "A2", "portrait": true, "scale": 50, "title": "{title}", "at": [0, 0]}},
        {{"op": "dim_scale", "scale": 50}},
        {{"op": "view", "id": "plan", "placed": "principal", "operations": [
            {{"op": "rectangle", "corner": [1500, 5000], "opposite": [8500, 20000]}}]}},
        {{"op": "view", "id": "aa", "placed": "section", "cut": {{"of": "plan", "from": [5000, 4000], "to": [5000, 21000], "toward": [6000, 12000]}}, "operations": [
            {{"op": "rectangle", "corner": [0, 0], "opposite": [15000, 3000]}}]}},
        {{"op": "notes", "title": "NOTES", "lines": ["Schematic."]}}
    "#
        ))
    };
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&floor("GROUND FLOOR"))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let first = h.entities();
    let ran = h
        .session
        .run_intent(&floor("UPPER FLOOR"))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    assert!(
        ran[0]
            .log
            .iter()
            .any(|l| l == "The sheet is drawn to the right of what the drawing held, clear of it."),
        "{:?}",
        ran[0].log
    );
    // The first sheet as it was.
    let now = h.entities();
    for entity in &first {
        assert!(now.contains(entity), "{entity:?} moved");
    }
    let drawing = h.session.drawing();
    let frame = |name: &str| {
        let group = drawing
            .groups()
            .iter()
            .find(|(_, g)| g.name == name)
            .unwrap()
            .1;
        crate::titleblock::SheetFrame::read(&group.description)
            .unwrap()
            .border
    };
    let (one, two) = (frame("TITLE-1"), frame("TITLE-2"));
    assert!(!one.intersects(&two), "{one:?} {two:?}");
    // Each sheet's views in its own groups, inside its own border.
    for (sheet, border) in [("TITLE-1", one), ("TITLE-2", two)] {
        for name in ["plan", "aa", "notes"] {
            let group = crate::compose::view_group(sheet, name);
            let members = &drawing
                .groups()
                .iter()
                .find(|(_, g)| g.name == group)
                .unwrap_or_else(|| panic!("no {group}"))
                .1
                .members;
            for id in members {
                let b = drawing.entity(*id).unwrap().bounds(drawing.tables());
                assert!(
                    border.contains_point(b.min()) && border.contains_point(b.max()),
                    "{group}: {b:?} outside {border:?}"
                );
            }
        }
        let mark = crate::compose::section_group(sheet, 'A');
        assert!(
            drawing.groups().iter().any(|(_, g)| g.name == mark),
            "no {mark}"
        );
    }
    assert_eq!(crate::compose::view_group("TITLE-2", "plan"), "VIEW-plan$2");
}

/// One stair in every view (#358): the house's plans and sections drew
/// four different stairs. One `id`, one height, width, and kind: the plan's
/// run along the climb is the section's, and the cut across is as wide as
/// the plan's two flights; another height for the same id is refused.
#[test]
fn one_stair_by_its_id_agrees_in_plan_along_and_across() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.session
        .run_intent(&document(
            r#"
        {"op": "stair", "id": "S1", "height": 3200, "width": 1200, "kind": "u", "view": "plan", "at": [0, 0]},
        {"op": "stair", "id": "S1", "height": 3200, "width": 1200, "kind": "u", "view": "section", "at": [20000, 0]},
        {"op": "stair", "id": "S1", "height": 3200, "width": 1200, "kind": "u", "view": "across", "cut": 1800, "at": [40000, 0]}
    "#,
        ))
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    let extent = |lo: f64, hi: f64| {
        let boxes: Vec<cad_core::Aabb> = h
            .entities()
            .iter()
            .filter(|e| matches!(e, Entity::Polyline(_)))
            .map(|e| e.bounds(h.session.drawing().tables()))
            .filter(|b| b.min().x >= lo && b.max().x < hi)
            .collect();
        boxes.into_iter().reduce(|a, b| a.union(&b)).unwrap()
    };
    let plan = extent(-10000.0, 10000.0);
    let section = extent(15000.0, 30000.0);
    let across = extent(35000.0, 50000.0);
    // The plan climbs along +y; the section runs along +x, its cut from
    // the foot to the landing's far side, and its floors a metre beyond.
    let along = plan.max().y - plan.min().y;
    let cut = h
        .entities()
        .iter()
        .filter_map(|e| match e {
            Entity::Polyline(p)
                if p.closed
                    && p.vertices
                        .first()
                        .is_some_and(|v| (19000.0..30000.0).contains(&v.x)) =>
            {
                Some(e.bounds(h.session.drawing().tables()))
            }
            _ => None,
        })
        .reduce(|a, b| a.union(&b))
        .unwrap();
    assert!(
        (cut.max().x - cut.min().x - along).abs() < 1e-6,
        "{cut:?} {plan:?}"
    );
    assert!(section.max().y > 3199.0);
    // Across: both flights, side by side, as wide as the plan.
    let wide = plan.max().x - plan.min().x;
    assert!(
        (across.max().x - across.min().x - wide - 600.0).abs() < 1e-6,
        "{across:?} {plan:?}"
    );

    // The same id with another height is another stair: refused.
    let refused = parse(&format!(
        r#"{{"version": {VERSION}, "operations": [
        {{"op": "stair", "id": "S1", "height": 3200, "width": 1200, "kind": "u", "at": [0, 0]}},
        {{"op": "view", "id": "bb", "placed": "free", "operations": [
            {{"op": "stair", "id": "S1", "height": 3000, "width": 1200, "kind": "u", "view": "across", "at": [0, 0]}}]}}]}}"#
    ))
    .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("names stair `S1`, drawn by operation 0 with height 3200"),
        "{refused}"
    );
}
