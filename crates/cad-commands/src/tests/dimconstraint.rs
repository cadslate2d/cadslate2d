//! DIMCONSTRAINT and -PARAMETERS through the session (#268): each form set
//! at its measured value or a new one, the geometry following as one undo,
//! a value edited by name, and over-constraint refused.

use cad_core::{EntityId, Point2};
use cad_document::{Axis, CircleEntity, Entity, Kind, LineEntity};

use crate::testing::Harness;

fn line(h: &Harness, id: EntityId) -> LineEntity {
    *h.session
        .drawing()
        .entity(id)
        .and_then(Entity::as_line)
        .expect("a line")
}

fn id(n: u64) -> EntityId {
    EntityId::from_raw(n)
}

fn constraint(h: &Harness, name: &str) -> Kind {
    let constraints = h.session.drawing().constraints();
    constraints
        .get(constraints.find(name).expect(name))
        .unwrap()
        .kind
        .clone()
}

/// A line along x, 120 long, its start fixed: a distance on it moves its
/// end.
fn fixed_line() -> Harness {
    let mut h = Harness::with_entities([Entity::Line(LineEntity::new(
        Point2::ORIGIN,
        Point2::new(120.0, 0.0),
    ))]);
    h.enter_all(&["GCFIX", "0,0"]);
    h.enter("GCHORIZONTAL");
    h.click_world(Point2::new(60.0, 0.0));
    h
}

#[test]
fn a_linear_constraint_at_its_measure_changes_nothing_and_a_new_value_moves_the_end() {
    let mut h = fixed_line();
    h.enter("DIMCONSTRAINT");
    assert_eq!(
        h.prompt(),
        "Enter constraint form or [LInear/Horizontal/Vertical/ALigned/ANgular/Radial/Diameter] <ALigned>:"
    );
    h.enter_all(&["LI", "0,0", "120,0", "60,-15"]);
    assert_eq!(h.prompt(), "Enter value for d1 <120>:");
    let before = h.entities();
    h.enter("");
    assert_eq!(h.entities(), before, "the measured value moves nothing");
    assert_eq!(
        constraint(&h, "d1"),
        Kind::Distance {
            axis: Axis::Horizontal,
            value: 120.0
        }
    );
    assert_eq!(h.log().last().map(String::as_str), Some("d1 = 120"));
    assert_eq!(
        h.session
            .drawing()
            .constraints()
            .iter()
            .last()
            .unwrap()
            .1
            .place,
        Some(Point2::new(60.0, -15.0))
    );

    // A second one, at a new value: d2, and the end follows.
    let mut h = fixed_line();
    h.enter_all(&["DCHORIZONTAL", "0,0", "120,0", "60,-15", "150"]);
    assert!((line(&h, id(1)).end.x - 150.0).abs() < 1e-7);
    // One undo takes back the constraint and the move.
    h.undo();
    assert_eq!(line(&h, id(1)).end, Point2::new(120.0, 0.0));
    assert!(h.session.drawing().constraints().find("d1").is_none());
}

#[test]
fn vertical_and_aligned_measure_points_and_a_line_by_the_object_option() {
    let mut h = Harness::with_entities([Entity::Line(LineEntity::new(
        Point2::ORIGIN,
        Point2::new(30.0, 40.0),
    ))]);
    h.enter_all(&["GCFIX", "0,0"]);
    // The Object option: the line's two ends; aligned, 50 → 100.
    h.enter_all(&["DCALIGNED", ""]);
    assert_eq!(h.prompt(), "Select object:");
    h.click_world(Point2::new(15.0, 20.0));
    h.enter_all(&["-10,30", "100"]);
    let stretched = line(&h, id(1));
    assert!((stretched.start.distance_to(stretched.end) - 100.0).abs() < 1e-7);
    assert!(
        stretched.start.distance_to(Point2::ORIGIN) < 1e-9,
        "the fixed start"
    );

    let mut h = Harness::with_entities([Entity::Line(LineEntity::new(
        Point2::ORIGIN,
        Point2::new(0.0, 40.0),
    ))]);
    h.enter_all(&["GCFIX", "0,0", "DCVERTICAL", "0,0", "0,40", "10,20", "60"]);
    assert!((line(&h, id(1)).end.y - 60.0).abs() < 1e-7);
    assert_eq!(
        constraint(&h, "d1"),
        Kind::Distance {
            axis: Axis::Vertical,
            value: 60.0
        }
    );
}

#[test]
fn angular_radial_and_diameter_drive_their_geometry() {
    let mut h = Harness::with_entities([
        Entity::Line(LineEntity::new(Point2::ORIGIN, Point2::new(100.0, 0.0))),
        Entity::Line(LineEntity::new(Point2::ORIGIN, Point2::new(50.0, 50.0))),
        Entity::Circle(CircleEntity::new(Point2::new(200.0, 0.0), 10.0)),
        Entity::Circle(CircleEntity::new(Point2::new(300.0, 0.0), 10.0)),
    ]);
    h.enter_all(&["GCFIX", "0,0", "GCHORIZONTAL"]);
    h.click_world(Point2::new(50.0, 0.0));
    h.enter_all(&["GCCOINCIDENT", "0,0", "0,0"]);
    h.enter("DCANGULAR");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(25.0, 25.0));
    h.enter("30,10");
    assert_eq!(h.prompt(), "Enter value for ang1 <45>:");
    h.enter("30");
    let turned = line(&h, id(2));
    let angle = (turned.end.y - turned.start.y)
        .atan2(turned.end.x - turned.start.x)
        .to_degrees();
    assert!((angle - 30.0).abs() < 1e-6, "{angle}");

    h.enter("DCRADIUS");
    h.click_world(Point2::new(210.0, 0.0));
    h.enter_all(&["220,10", "15"]);
    h.enter("DCDIAMETER");
    h.click_world(Point2::new(310.0, 0.0));
    h.enter_all(&["320,10", "50"]);
    let radius = |n| match h.session.drawing().entity(id(n)) {
        Some(Entity::Circle(c)) => c.radius,
        other => panic!("{other:?}"),
    };
    assert!((radius(3) - 15.0).abs() < 1e-9);
    assert!((radius(4) - 25.0).abs() < 1e-9);
    assert_eq!(constraint(&h, "rad1"), Kind::Radius { value: 15.0 });
    assert_eq!(constraint(&h, "dia1"), Kind::Diameter { value: 50.0 });

    // A line is not a circle, a circle not a line.
    h.enter("DCRADIUS");
    h.click_world(Point2::new(50.0, 0.0));
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Pick an arc or a circle.")
    );
}

#[test]
fn parameters_edits_renames_deletes_and_lists_by_name_as_one_undo() {
    let mut h = fixed_line();
    h.enter_all(&["DCHORIZONTAL", "0,0", "120,0", "60,-15", ""]);
    h.enter_all(&["-PARAMETERS", "E", "d1", "90"]);
    assert!((line(&h, id(1)).end.x - 90.0).abs() < 1e-7);
    assert_eq!(h.log().last().map(String::as_str), Some("d1 = 90"));
    h.undo();
    assert!((line(&h, id(1)).end.x - 120.0).abs() < 1e-7);

    h.enter_all(&["PAR", "R", "D1", "width"]);
    assert!(h.session.drawing().constraints().find("width").is_some());
    h.enter_all(&["PAR", "?"]);
    assert_eq!(h.log().last().map(String::as_str), Some("width = 120"));
    h.enter_all(&["PAR", "E", "nothing"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("No parameter called \"nothing\".")
    );
    h.enter_all(&["PAR", "D", "width"]);
    assert!(h.session.drawing().constraints().find("width").is_none());
    assert_eq!(h.entities().len(), 1, "the geometry stays");
}

#[test]
fn a_dimension_the_others_already_determine_is_refused() {
    let mut h = fixed_line();
    h.enter_all(&["DCHORIZONTAL", "0,0", "120,0", "60,-15", ""]);
    // The same length again, aligned: already determined.
    h.enter_all(&["DCALIGNED", "0,0", "120,0", "60,15", ""]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The geometry is already fully determined there: it would over-constrain it.")
    );
    assert_eq!(h.session.drawing().constraints().len(), 3);
    // Determined, and at another value: a contradiction.
    h.enter_all(&["DCALIGNED", "0,0", "120,0", "60,15", "150"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("It contradicts the other constraints.")
    );
    assert_eq!(line(&h, id(1)).end, Point2::new(120.0, 0.0));
    assert!(h.session.drawing().constraints().find("d2").is_none());
}
