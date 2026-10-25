//! The part generators through the session (#272): each command's prompts
//! and defaults, the part made constrained as one undo and previewed
//! before, impossible parts refused with the reason, and the same part by
//! command and by the intent IR.

use cad_core::Point2;
use cad_document::{Constraint, Entity};
use cad_intent::parse;

use crate::OverlayItem;
use crate::testing::Harness;

fn kinds(h: &Harness) -> Vec<&'static str> {
    h.entities().iter().map(Entity::kind_name).collect()
}

fn constraints(h: &Harness) -> Vec<Constraint> {
    h.session
        .drawing()
        .constraints()
        .iter()
        .map(|(_, c)| c.clone())
        .collect()
}

fn made(h: &Harness) {
    assert!(
        h.session.drawing().unsatisfied_constraints().is_empty(),
        "{:?}",
        h.log()
    );
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log());
}

#[test]
fn plate_by_its_corners_or_its_dimensions_with_corners_and_holes() {
    let mut h = Harness::new();
    h.enter("PLATE");
    assert_eq!(h.prompt(), "Specify first corner:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify other corner or [Dimensions]:");
    h.enter("120,80");
    assert_eq!(
        h.prompt(),
        "Enter corner type or [Square/Fillet/Chamfer] <Square>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Specify corner hole diameter or [None] <None>:");
    h.enter("");
    made(&h);
    assert_eq!(kinds(&h), ["line", "line", "line", "line"]);
    assert_eq!(
        constraints(&h).len(),
        10,
        "four corners, two levels, two plumbs, the width and height"
    );
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Plate: 4 objects, 10 constraints.")
    );
    // One undo takes the part away whole.
    h.undo();
    assert!(h.entities().is_empty());
    assert!(constraints(&h).is_empty());

    // By its dimensions, filleted, with holes.
    h.enter_all(&["PLATE", "10,10", "D", "120", "80", "F", "5", "8", "10"]);
    made(&h);
    assert_eq!(
        kinds(&h),
        [
            "line", "line", "line", "line", "arc", "arc", "arc", "arc", "circle", "circle",
            "circle", "circle"
        ]
    );
}

#[test]
fn slot_hole_pattern_and_bolt_circle_take_their_defaults() {
    let mut h = Harness::new();
    h.enter_all(&["SLOT", "0,0", "", "", ""]);
    made(&h);
    assert_eq!(kinds(&h), ["line", "line", "arc", "arc"]);

    let mut h = Harness::new();
    h.enter_all(&["HOLEPATTERN", "0,0", "6", "2", "3", "20", "15"]);
    made(&h);
    assert_eq!(kinds(&h).len(), 6);
    // One row: no row spacing asked.
    let mut h = Harness::new();
    h.enter_all(&["HOLEPATTERN", "0,0", "6", "1", "3"]);
    assert_eq!(h.prompt(), "Specify column spacing <25>:");
    h.enter("15");
    made(&h);
    assert_eq!(kinds(&h).len(), 3);

    let mut h = Harness::new();
    h.enter_all(&["BOLTCIRCLE", "0,0", "100", "6", "10", "0"]);
    made(&h);
    assert_eq!(kinds(&h).len(), 7, "the pitch circle and six holes");
    // A count is a whole number.
    let mut h = Harness::new();
    h.enter_all(&["BOLTCIRCLE", "0,0", "100", "2.5"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Enter a whole number, 1 or more.")
    );
}

#[test]
fn flanges_round_and_rectangular() {
    let mut h = Harness::new();
    h.enter_all(&["FLANGE", "", "0,0", "150", "50", "110", "8", "12"]);
    made(&h);
    assert_eq!(
        kinds(&h).len(),
        11,
        "pitch circle, eight holes, outside, bore"
    );

    let mut h = Harness::new();
    h.enter_all(&["FLANGE", "RE", "0,0", "120", "100", "40", "10", "15"]);
    made(&h);
    assert_eq!(
        kinds(&h),
        [
            "line", "line", "line", "line", "circle", "circle", "circle", "circle", "circle"
        ]
    );
}

#[test]
fn an_impossible_part_is_refused_with_the_reason_and_nothing_made() {
    let mut h = Harness::new();
    let revision = h.session.drawing().revision();
    h.enter_all(&["PLATE", "0,0", "D", "100", "50", "F", "30", ""]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The fillet radius is too large for the plate.")
    );
    assert!(h.entities().is_empty());
    h.enter_all(&["BOLTCIRCLE", "0,0", "20", "12", "10", "0"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The holes would run into each other: fewer, smaller, or a larger pitch circle.")
    );
    assert!(h.entities().is_empty());
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_part_is_previewed_before_it_is_made() {
    let mut h = Harness::new();
    let revision = h.session.drawing().revision();
    h.enter_all(&["PLATE", "0,0"]);
    h.move_to_world(Point2::new(100.0, 60.0));
    let previews = h
        .session
        .overlay()
        .into_iter()
        .filter(|item| matches!(item, OverlayItem::PreviewEntity(Entity::Line(_))))
        .count();
    assert_eq!(previews, 4);
    assert_eq!(h.session.drawing().revision(), revision);
}

/// The same part by command and by the intent IR makes the same drawing:
/// the entities and the constraints, names and all.
#[test]
fn the_same_part_by_command_and_by_intent_is_the_same_drawing() {
    let cases: [(&[&str], &str); 6] = [
        (
            &["PLATE", "0,0", "D", "120", "80", "C", "4", "8", "10"],
            r#"{"op": "plate", "corner": [0, 0], "width": 120, "height": 80, "chamfer": 4, "hole_diameter": 8, "hole_offset": 10}"#,
        ),
        (
            &["SLOT", "5,5", "40", "10", "30"],
            r#"{"op": "slot", "center": [5, 5], "length": 40, "width": 10, "angle": 30}"#,
        ),
        (
            &["HOLEPATTERN", "0,0", "6", "1", "4", "15"],
            r#"{"op": "hole_pattern", "first": [0, 0], "diameter": 6, "rows": 1, "columns": 4, "column_spacing": 15}"#,
        ),
        (
            &["BOLTCIRCLE", "0,0", "100", "6", "10", "15"],
            r#"{"op": "bolt_circle", "center": [0, 0], "pitch_diameter": 100, "count": 6, "hole_diameter": 10, "start_angle": 15}"#,
        ),
        (
            &["FLANGE", "Round", "0,0", "150", "50", "110", "8", "12"],
            r#"{"op": "round_flange", "center": [0, 0], "outer_diameter": 150, "bore_diameter": 50, "pitch_diameter": 110, "count": 8, "hole_diameter": 12}"#,
        ),
        (
            &[
                "FLANGE",
                "REctangular",
                "0,0",
                "120",
                "100",
                "40",
                "10",
                "15",
            ],
            r#"{"op": "rectangular_flange", "center": [0, 0], "width": 120, "height": 100, "bore_diameter": 40, "hole_diameter": 10, "hole_offset": 15}"#,
        ),
    ];
    for (typed, operation) in cases {
        let mut by_command = Harness::new();
        by_command.enter_all(typed);
        made(&by_command);
        let mut by_intent = Harness::new();
        let intent = parse(&format!(r#"{{"version": 2, "operations": [{operation}]}}"#)).unwrap();
        by_intent.session.run_intent(&intent).unwrap();
        made(&by_intent);
        assert_eq!(by_intent.entities(), by_command.entities(), "{operation}");
        assert_eq!(
            constraints(&by_intent),
            constraints(&by_command),
            "{operation}"
        );
    }
}
