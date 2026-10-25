//! GEOMCONSTRAINT and DELCONSTRAINT through the session (#267): each kind
//! on each kind of entity it takes, the constrained geometry settled in
//! the same undo, the first object staying and the second coming to it,
//! and the refusals.

use cad_core::{EntityId, Point2};
use cad_document::{
    ArcEntity, CircleEntity, Entity, Kind, LineEntity, Part, PolylineEntity, Reference,
};

use crate::testing::Harness;

fn near(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-6
}

fn line(h: &Harness, id: EntityId) -> LineEntity {
    *h.session
        .drawing()
        .entity(id)
        .and_then(Entity::as_line)
        .expect("a line")
}

fn circle(h: &Harness, id: EntityId) -> CircleEntity {
    match h.session.drawing().entity(id) {
        Some(Entity::Circle(c)) => *c,
        other => panic!("{other:?}"),
    }
}

fn kinds(h: &Harness) -> Vec<Kind> {
    h.session
        .drawing()
        .constraints()
        .iter()
        .map(|(_, c)| c.kind.clone())
        .collect()
}

fn id(n: u64) -> EntityId {
    EntityId::from_raw(n)
}

/// A line by its ends' coordinates.
type Ends = ((f64, f64), (f64, f64));

/// Lines from point pairs; their IDs are 1, 2, … in order.
fn lines(pairs: &[Ends]) -> Harness {
    Harness::with_entities(pairs.iter().map(|&((x0, y0), (x1, y1))| {
        Entity::Line(LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    }))
}

#[test]
fn horizontal_and_vertical_level_a_line_a_segment_or_two_points_in_one_undo() {
    let mut h = lines(&[((0.0, 0.0), (100.0, 10.0)), ((200.0, 0.0), (205.0, 80.0))]);
    h.enter("GCHORIZONTAL");
    assert_eq!(h.prompt(), "Select an object or [2Points] <2Points>:");
    h.click_world(Point2::new(50.0, 5.0));
    assert_eq!(h.prompt(), "Command:");
    let level = line(&h, id(1));
    assert!((level.start.y - level.end.y).abs() < 1e-9, "{level:?}");
    assert_eq!(kinds(&h), [Kind::Horizontal]);
    // One undo takes back the constraint and the move together.
    h.undo();
    assert_eq!(line(&h, id(1)).end, Point2::new(100.0, 10.0));
    assert!(kinds(&h).is_empty());

    // Two points: the second line's ends made plumb.
    h.enter_all(&["GCVERTICAL", "2P", "200,0", "205,80"]);
    let plumb = line(&h, id(2));
    assert!((plumb.start.x - plumb.end.x).abs() < 1e-9, "{plumb:?}");

    // A polyline's straight segment.
    let mut h = Harness::with_entities([Entity::Polyline(PolylineEntity::new(
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(50.0, 3.0),
            Point2::new(60.0, 40.0),
        ],
        false,
    ))]);
    h.enter("GEOMCONSTRAINT");
    assert_eq!(
        h.prompt(),
        "Enter constraint type or [Horizontal/Vertical/Perpendicular/PArallel/Tangent/SMooth/Coincident/CONcentric/COLlinear/Symmetric/Equal/Fix] <Coincident>:"
    );
    h.enter("H");
    h.click_world(Point2::new(25.0, 1.5));
    let Some(Entity::Polyline(p)) = h.session.drawing().entity(id(1)) else {
        panic!()
    };
    assert!((p.vertices[0].y - p.vertices[1].y).abs() < 1e-9);
    assert_eq!(
        h.session
            .drawing()
            .constraints()
            .iter()
            .next()
            .unwrap()
            .1
            .references,
        [Reference::new(id(1), Part::Segment(0))]
    );
}

#[test]
fn parallel_perpendicular_and_collinear_turn_the_second_line_to_the_first() {
    let mut h = lines(&[
        ((0.0, 0.0), (100.0, 0.0)),
        ((0.0, 20.0), (100.0, 30.0)),
        ((0.0, 50.0), (10.0, 90.0)),
        ((150.0, 3.0), (250.0, 3.0)),
    ]);
    h.enter("GCPARALLEL");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(50.0, 25.0));
    assert_eq!(
        line(&h, id(1)).end,
        Point2::new(100.0, 0.0),
        "the first stays"
    );
    let second = line(&h, id(2));
    assert!((second.start.y - second.end.y).abs() < 1e-9, "{second:?}");

    h.enter("GCPERPENDICULAR");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(5.0, 70.0));
    let third = line(&h, id(3));
    assert!((third.start.x - third.end.x).abs() < 1e-9, "{third:?}");

    h.enter("GCCOLLINEAR");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(200.0, 3.0));
    let fourth = line(&h, id(4));
    assert!(
        fourth.start.y.abs() < 1e-9 && fourth.end.y.abs() < 1e-9,
        "{fourth:?}"
    );
    assert_eq!(
        kinds(&h),
        [Kind::Parallel, Kind::Perpendicular, Kind::Collinear]
    );
}

#[test]
fn coincident_brings_the_second_point_to_the_first_or_a_point_onto_an_object() {
    let mut h = lines(&[((0.0, 0.0), (100.0, 0.0)), ((104.0, 3.0), (104.0, 60.0))]);
    h.enter("GCCOINCIDENT");
    assert_eq!(h.prompt(), "Select first point or [Object] <Object>:");
    h.enter_all(&["100,0", "104,3"]);
    assert!(near(line(&h, id(2)).start, Point2::new(100.0, 0.0)));
    assert_eq!(line(&h, id(2)).end, Point2::new(104.0, 60.0));
    assert_eq!(line(&h, id(1)).end, Point2::new(100.0, 0.0));

    // A point onto a circle: the Object option.
    let mut h = Harness::with_entities([
        Entity::Circle(CircleEntity::new(Point2::ORIGIN, 20.0)),
        Entity::Line(LineEntity::new(
            Point2::new(30.0, 0.0),
            Point2::new(60.0, 0.0),
        )),
    ]);
    h.enter_all(&["GCCOINCIDENT", "O"]);
    assert_eq!(h.prompt(), "Select first object:");
    h.click_world(Point2::new(0.0, 20.0));
    h.enter("30,0");
    let moved = line(&h, id(2));
    assert!(
        (moved.start.distance_to(Point2::ORIGIN) - 20.0).abs() < 1e-7,
        "{moved:?}"
    );
    assert_eq!(circle(&h, id(1)).radius, 20.0);

    // Nothing there to take.
    h.enter_all(&["GCCOINCIDENT", "500,500"]);
    assert!(
        h.log()
            .iter()
            .any(|l| l == "No end, centre, or vertex there.")
    );
}

#[test]
fn concentric_equal_and_tangent_on_circles_and_arcs() {
    let mut h = Harness::with_entities([
        Entity::Circle(CircleEntity::new(Point2::ORIGIN, 10.0)),
        Entity::Circle(CircleEntity::new(Point2::new(3.0, 4.0), 25.0)),
        Entity::Arc(ArcEntity::new(Point2::new(100.0, 0.0), 8.0, 0.0, 3.0)),
        Entity::Line(LineEntity::new(
            Point2::new(-50.0, -30.0),
            Point2::new(50.0, -30.0),
        )),
    ]);
    h.enter("GCCONCENTRIC");
    h.click_world(Point2::new(10.0, 0.0));
    h.click_world(Point2::new(28.0, 4.0));
    assert!(near(circle(&h, id(2)).center, Point2::ORIGIN));
    assert!(
        near(circle(&h, id(1)).center, Point2::ORIGIN),
        "the first stays"
    );

    // Equal radii: the arc takes the first circle's.
    h.enter("GCEQUAL");
    h.click_world(Point2::new(10.0, 0.0));
    h.click_world(Point2::new(108.0, 0.0));
    let Some(Entity::Arc(arc)) = h.session.drawing().entity(id(3)) else {
        panic!()
    };
    assert!((arc.radius - 10.0).abs() < 1e-7, "{arc:?}");

    // Tangent: the line, below the circle, comes up to touch it.
    h.enter("GCTANGENT");
    h.click_world(Point2::new(0.0, 10.0));
    h.click_world(Point2::new(0.0, -30.0));
    let touching = line(&h, id(4));
    assert!((touching.start.y + 10.0).abs() < 1e-6, "{touching:?}");
    assert!(
        h.session
            .drawing()
            .constraints()
            .iter()
            .any(|(_, c)| matches!(
                c.kind,
                Kind::Tangent {
                    touch: cad_document::Touch::Right | cad_document::Touch::Left
                }
            ))
    );
}

#[test]
fn symmetric_mirrors_two_points_in_a_line_and_fix_keeps_one() {
    let mut h = lines(&[
        ((-20.0, 10.0), (-20.0, 40.0)),
        ((25.0, 12.0), (25.0, 50.0)),
        ((0.0, -10.0), (0.0, 100.0)),
    ]);
    h.enter("GCSYMMETRIC");
    h.enter_all(&["-20,10", "25,12"]);
    assert_eq!(h.prompt(), "Select symmetry line:");
    h.click_world(Point2::new(0.0, 50.0));
    let (a, b) = (line(&h, id(1)).start, line(&h, id(2)).start);
    assert!(
        (a.x + b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6,
        "{a:?} {b:?}"
    );

    // Fixed where it is: an edit that would move it is refused.
    h.enter_all(&["GCFIX", "0,-10"]);
    assert!(matches!(
        kinds(&h).last(),
        Some(Kind::Fix { at }) if *at == Point2::new(0.0, -10.0)
    ));
    let before = h.session.drawing().revision();
    h.enter("MOVE");
    h.click_world(Point2::new(0.0, 50.0));
    h.enter_all(&["", "0,0", "10,0"]);
    assert_eq!(h.session.drawing().revision(), before, "{:?}", h.log());
    assert!(
        h.log()
            .iter()
            .any(|l| l.starts_with("The edit would break") && l.contains("fix")),
        "{:?}",
        h.log()
    );
}

#[test]
fn arcs_ends_and_point_objects_are_points_to_constrain() {
    let mut h = Harness::with_entities([
        Entity::Arc(ArcEntity::new(
            Point2::ORIGIN,
            10.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        )),
        Entity::Line(LineEntity::new(
            Point2::new(12.0, 1.0),
            Point2::new(40.0, 1.0),
        )),
        Entity::Point(cad_document::PointEntity::new(Point2::new(70.0, 70.0))),
    ]);
    // The line's start to the arc's start, at (10, 0).
    h.enter_all(&["GCCOINCIDENT", "10,0", "12,1"]);
    assert!(near(line(&h, id(2)).start, Point2::new(10.0, 0.0)));
    assert_eq!(
        h.session
            .drawing()
            .constraints()
            .iter()
            .next()
            .unwrap()
            .1
            .references,
        [
            Reference::new(id(1), Part::Start),
            Reference::new(id(2), Part::Start)
        ]
    );
    // A point object fixed.
    h.enter_all(&["GCFIX", "70,70"]);
    assert!(matches!(
        kinds(&h).last(),
        Some(Kind::Fix { at }) if *at == Point2::new(70.0, 70.0)
    ));
}

#[test]
fn what_cannot_be_constrained_is_refused_and_says_why() {
    let mut h = Harness::with_entities([
        Entity::Line(LineEntity::new(Point2::ORIGIN, Point2::new(100.0, 0.0))),
        Entity::Circle(CircleEntity::new(Point2::new(0.0, 50.0), 10.0)),
        Entity::Line(LineEntity::new(
            Point2::new(0.0, 100.0),
            Point2::new(100.0, 100.0),
        )),
    ]);
    // A circle is not a straight edge.
    h.enter("GCPARALLEL");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(10.0, 50.0));
    assert!(h.session.drawing().constraints().is_empty());
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Those are not the parts this constraint relates.")
    );
    // Smooth is for splines.
    h.enter_all(&["GEOMCONSTRAINT", "SM"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Smooth joins splines, which are not constrained.")
    );
    // Level twice: the second over-constrains.
    h.enter("GCHORIZONTAL");
    h.click_world(Point2::new(50.0, 0.0));
    h.enter("GCHORIZONTAL");
    h.click_world(Point2::new(50.0, 0.0));
    assert_eq!(h.session.drawing().constraints().len(), 1);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The other constraints already imply it: it would over-constrain the geometry.")
    );
    // Parallel, then perpendicular, between the same two lines: a
    // contradiction.
    h.enter("GCPARALLEL");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(50.0, 100.0));
    h.enter("GCPERPENDICULAR");
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(50.0, 100.0));
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("It contradicts the other constraints.")
    );
    assert_eq!(kinds(&h), [Kind::Horizontal, Kind::Parallel]);
}

#[test]
fn delconstraint_removes_every_constraint_of_the_selected_objects() {
    let mut h = lines(&[((0.0, 0.0), (100.0, 0.0)), ((100.0, 0.0), (100.0, 50.0))]);
    h.enter_all(&["GCCOINCIDENT", "100,0", "100,0"]);
    h.enter("GCHORIZONTAL");
    h.click_world(Point2::new(50.0, 0.0));
    h.enter("GCVERTICAL");
    h.click_world(Point2::new(100.0, 25.0));
    assert_eq!(h.session.drawing().constraints().len(), 3);
    let geometry = h.entities();

    // The vertical line: its coincidence and its plumb.
    h.enter("DELCON");
    h.click_world(Point2::new(100.0, 25.0));
    h.enter("");
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("2 constraints removed.")
    );
    assert_eq!(kinds(&h), [Kind::Horizontal]);
    assert_eq!(h.entities(), geometry, "the geometry stays");
    h.undo();
    assert_eq!(h.session.drawing().constraints().len(), 3);

    // Selected first, and with none on it.
    let mut h = lines(&[((0.0, 0.0), (1.0, 1.0))]);
    h.enter("ALL");
    h.enter("");
    h.enter("DELCONSTRAINT");
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The selected objects have no constraints.")
    );
}
