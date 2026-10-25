//! Placement (#273): CIRCLE Tan, Tan, Radius and Tan, Tan, Tan, TANARC,
//! and CONCENTRIC tangent to, or about, lines, arcs, and circles, kept so
//! by their constraints; ARRAYPATH along a line, an arc, and a circle;
//! ARRAYGRID inside a boundary. Each is one undo.

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Entity, Kind};
use cad_geometry::tangent::Curve;
use cad_geometry::{Circle, Segment};

use cad_intent::parse;

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn circles(h: &Harness) -> Vec<CircleEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Circle(circle) => Some(circle),
            _ => None,
        })
        .collect()
}

fn arcs(h: &Harness) -> Vec<ArcEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Arc(arc) => Some(arc),
            _ => None,
        })
        .collect()
}

fn kinds(h: &Harness) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = h
        .session
        .drawing()
        .constraints()
        .iter()
        .map(|(_, c)| match c.kind {
            Kind::Tangent { .. } => "tangent",
            Kind::Coincident => "coincident",
            Kind::Concentric => "concentric",
            Kind::Equal => "equal",
            _ => "other",
        })
        .collect();
    kinds.sort_unstable();
    kinds
}

fn holds(h: &Harness) {
    assert!(
        h.session.drawing().unsatisfied_constraints().is_empty(),
        "{:?}",
        h.log()
    );
}

fn near(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-6
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
    Curve::Line(Segment::new(p(x0, y0), p(x1, y1)))
}

fn touches(curve: Curve, circle: &CircleEntity) -> bool {
    curve.tangency_error(&Circle::new(circle.center, circle.radius)) < 1e-6
}

/// Two lines at a right angle, meeting at the origin.
fn corner() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "0,0", "0,100", ""]);
    h
}

#[test]
fn ttr_to_two_lines_is_kept_tangent_when_a_line_moves() {
    let mut h = corner();
    h.enter_all(&["CIRCLE", "T"]);
    assert_eq!(
        h.prompt(),
        "Specify point on object for first tangent of circle:"
    );
    h.click_world(p(50.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Specify point on object for second tangent of circle:"
    );
    h.click_world(p(0.0, 50.0));
    assert_eq!(h.prompt(), "Specify radius of circle:");
    h.enter("10");
    assert_eq!(h.prompt(), "Command:");
    let made = circles(&h);
    assert_eq!(made.len(), 1);
    assert!(near(made[0].center, p(10.0, 10.0)), "{made:?}");
    assert_eq!(kinds(&h), ["tangent", "tangent"]);
    holds(&h);

    // The bottom line up 5: the circle follows, still touching both —
    // the left line, free, may give a little.
    h.enter("MOVE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "0,5"]);
    holds(&h);
    let moved = circles(&h)[0];
    let lines: Vec<Curve> = h
        .entities()
        .iter()
        .filter_map(|e| {
            e.as_line()
                .map(|l| Curve::Line(Segment::new(l.start, l.end)))
        })
        .collect();
    assert_eq!(lines[0], line(0.0, 5.0, 100.0, 5.0));
    for l in lines {
        assert!(touches(l, &moved), "{moved:?} {l:?}");
    }

    // One undo each: the move, then the circle and its constraints.
    h.undo();
    h.undo();
    assert!(circles(&h).is_empty());
    assert!(h.session.drawing().constraints().is_empty());
}

#[test]
fn ttr_to_a_line_and_a_circle_and_to_a_line_and_an_arc() {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "-100,0", "100,0", ""]);
    h.enter_all(&["CIRCLE", "50,20", "10"]);
    h.enter_all(&["CIRCLE", "TTR"]);
    h.click_world(p(30.0, 0.0));
    h.click_world(p(42.0, 14.0));
    h.enter("10");
    let made = circles(&h);
    assert_eq!(made.len(), 2);
    assert!(
        near(made[1].center, p(50.0 - 300f64.sqrt(), 10.0)),
        "{made:?}"
    );
    assert_eq!(kinds(&h), ["tangent", "tangent"]);
    holds(&h);

    // Inside an arc, on its chord.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "-100,0", "100,0", ""]);
    h.enter_all(&["ARC", "50,0", "C", "0,0", "-50,0"]);
    assert_eq!(arcs(&h).len(), 1, "{:?}", h.log());
    h.enter_all(&["CIRCLE", "T"]);
    h.click_world(p(40.0, 0.0));
    h.click_world(p(45.0, 21.794_494_717_703_37));
    h.enter("10");
    let made = circles(&h);
    assert_eq!(made.len(), 1, "{:?}", h.log());
    assert!(near(made[0].center, p(1500f64.sqrt(), 10.0)), "{made:?}");
    assert_eq!(kinds(&h), ["tangent", "tangent"]);
    holds(&h);
}

#[test]
fn ttr_in_the_corner_of_a_polyline_is_kept_tangent_to_its_segments() {
    let mut h = Harness::new();
    h.enter_all(&["RECTANG", "0,0", "100,60"]);
    h.enter_all(&["CIRCLE", "T"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(100.0, 30.0));
    h.enter("8");
    let made = circles(&h);
    assert!(near(made[0].center, p(92.0, 8.0)), "{made:?}");
    assert_eq!(kinds(&h), ["tangent", "tangent"]);
    holds(&h);
}

#[test]
fn ttt_inscribes_a_triangle_and_touches_three_circles() {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "40,0", "0,30", "C"]);
    h.enter_all(&["CIRCLE", "TTT"]);
    h.click_world(p(20.0, 0.0));
    h.click_world(p(0.0, 15.0));
    assert_eq!(
        h.prompt(),
        "Specify point on object for third tangent of circle:"
    );
    h.click_world(p(20.0, 15.0));
    let made = circles(&h);
    assert_eq!(made.len(), 1, "{:?}", h.log());
    assert!(near(made[0].center, p(10.0, 10.0)), "{made:?}");
    assert!((made[0].radius - 10.0).abs() < 1e-6);
    assert_eq!(kinds(&h), ["tangent", "tangent", "tangent"]);
    holds(&h);

    // Three equal circles at the corners of a triangle: the one between
    // them, picked on their inner sides.
    let mut h = Harness::new();
    let s3 = 3f64.sqrt();
    let centers = [p(0.0, 20.0), p(10.0 * s3, -10.0), p(-10.0 * s3, -10.0)];
    for c in centers {
        h.enter_all(&["CIRCLE", &format!("{},{}", c.x, c.y), "5"]);
    }
    h.enter_all(&["CIRCLE", "TTT"]);
    for c in centers {
        let inward = c - (c - Point2::ORIGIN) * (5.0 / 20.0);
        h.click_world(inward);
    }
    let made = circles(&h);
    assert_eq!(made.len(), 4, "{:?}", h.log());
    assert!(near(made[3].center, p(0.0, 0.0)), "{:?}", made[3]);
    assert!((made[3].radius - 15.0).abs() < 1e-6, "{:?}", made[3]);
    holds(&h);
}

#[test]
fn a_circle_that_cannot_touch_both_does_not_exist() {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "0,20", "100,20", ""]);
    let revision = h.session.drawing().revision();
    h.enter_all(&["CIRCLE", "T"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("5");
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Circle does not exist.")
    );
    assert_eq!(h.session.drawing().revision(), revision);
    // A radius of half the gap fits.
    h.enter_all(&["CIRCLE", "T"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 20.0));
    h.enter("10");
    assert_eq!(circles(&h).len(), 1);
    holds(&h);
}

#[test]
fn tanarc_fillets_without_trimming_and_keeps_its_ends_on_the_lines() {
    let mut h = corner();
    h.enter("TANARC");
    assert_eq!(h.prompt(), "Select first object for the arc to touch:");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 50.0));
    assert_eq!(h.prompt(), "Specify radius of arc:");
    h.enter("10");
    let made = arcs(&h);
    assert_eq!(made.len(), 1, "{:?}", h.log());
    let arc = made[0].arc();
    assert!(near(arc.center, p(10.0, 10.0)));
    let ends = [arc.start_point(), arc.end_point()];
    assert!(ends.iter().any(|e| near(*e, p(10.0, 0.0))), "{ends:?}");
    assert!(ends.iter().any(|e| near(*e, p(0.0, 10.0))), "{ends:?}");
    assert!(
        (arc.sweep.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "the short way round"
    );
    // Nothing trimmed.
    assert_eq!(h.entities().len(), 3);
    assert_eq!(
        kinds(&h),
        ["coincident", "coincident", "tangent", "tangent"]
    );
    holds(&h);

    // The bottom line moved up: the arc follows, its ends on both lines.
    h.enter("MOVE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "0,4"]);
    holds(&h);
    let arc = arcs(&h)[0].arc();
    assert!(
        [arc.start_point(), arc.end_point()]
            .iter()
            .any(|e| (e.y - 4.0).abs() < 1e-6),
        "{arc:?}"
    );

    // An arc tangent to a line and a circle.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "-100,0", "100,0", ""]);
    h.enter_all(&["CIRCLE", "50,20", "10"]);
    h.enter_all(&["TANARC"]);
    h.click_world(p(30.0, 0.0));
    h.click_world(p(42.0, 14.0));
    h.enter("10");
    assert_eq!(arcs(&h).len(), 1, "{:?}", h.log());
    holds(&h);
}

#[test]
fn concentric_keeps_its_centre_with_the_circle_or_arc() {
    let mut h = Harness::new();
    h.enter_all(&["CIRCLE", "0,0", "20"]);
    h.enter("CONCENTRIC");
    assert_eq!(h.prompt(), "Select circle or arc to be concentric with:");
    h.click_world(p(20.0, 0.0));
    assert_eq!(h.prompt(), "Specify radius of circle or [Diameter]:");
    h.enter("30");
    let made = circles(&h);
    assert_eq!(made[1], CircleEntity::new(p(0.0, 0.0), 30.0));
    assert_eq!(kinds(&h), ["concentric"]);
    // The first circle moved: the second goes with it.
    h.enter("MOVE");
    h.click_world(p(20.0, 0.0));
    h.enter_all(&["", "0,0", "5,5"]);
    holds(&h);
    assert!(near(circles(&h)[1].center, p(5.0, 5.0)));

    // About an arc, by its diameter; a line is refused.
    let mut h = Harness::new();
    h.enter_all(&["ARC", "50,0", "C", "0,0", "0,50"]);
    h.enter_all(&["LINE", "100,0", "200,0", ""]);
    h.enter("CONCENTRIC");
    h.click_world(p(150.0, 0.0));
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("Select a circle or an arc.")
    );
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["D", "40"]);
    assert_eq!(circles(&h), [CircleEntity::new(p(0.0, 0.0), 20.0)]);
    holds(&h);
}

#[test]
fn arraypath_spreads_items_along_a_line_and_keeps_circles_on_it() {
    let mut h = Harness::new();
    h.enter_all(&["CIRCLE", "-50,-50", "3"]);
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.click_world(p(-47.0, -50.0));
    h.enter("ARRAYPATH");
    assert_eq!(h.prompt(), "Select path curve:");
    h.click_world(p(10.0, 0.0));
    assert_eq!(h.prompt(), "Enter number of items along path or [Measure]:");
    h.enter("5");
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("5 items along the path.")
    );
    let centers: Vec<Point2> = circles(&h).iter().map(|c| c.center).collect();
    assert_eq!(centers.len(), 5);
    for (center, x) in centers.iter().zip([0.0, 25.0, 50.0, 75.0, 100.0]) {
        assert!(near(*center, p(x, 0.0)), "{centers:?}");
    }
    assert_eq!(
        kinds(&h),
        [
            "coincident",
            "coincident",
            "coincident",
            "coincident",
            "coincident",
            "equal",
            "equal",
            "equal",
            "equal"
        ]
    );
    holds(&h);
    // The path turned: the circles stay on it.
    h.enter("ROTATE");
    h.click_world(p(10.0, 0.0));
    h.enter_all(&["", "0,0", "30"]);
    holds(&h);
    // One undo each: the turn, then the array, the circle back in place.
    h.undo();
    h.undo();
    assert_eq!(circles(&h), [CircleEntity::new(p(-50.0, -50.0), 3.0)]);
    assert!(h.session.drawing().constraints().is_empty());
}

#[test]
fn arraypath_by_spacing_from_the_end_picked_and_round_a_circle() {
    let mut h = Harness::new();
    h.enter_all(&["CIRCLE", "-50,-50", "3"]);
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.click_world(p(-47.0, -50.0));
    // Picked near its far end: the items start there.
    h.enter_all(&["ARRAYPATH"]);
    h.click_world(p(90.0, 0.0));
    h.enter_all(&["M", "30"]);
    let xs: Vec<f64> = circles(&h).iter().map(|c| c.center.x).collect();
    assert_eq!(xs.len(), 4, "{xs:?}");
    for (x, expected) in xs.iter().zip([100.0, 70.0, 40.0, 10.0]) {
        assert!((x - expected).abs() < 1e-6, "{xs:?}");
    }
    holds(&h);

    // Round a circle: evenly, from its rightmost point.
    let mut h = Harness::new();
    h.enter_all(&["CIRCLE", "-50,-50", "3"]);
    h.enter_all(&["CIRCLE", "0,0", "40"]);
    h.click_world(p(-47.0, -50.0));
    h.enter("ARRAYPATH");
    h.click_world(p(40.0, 0.0));
    h.enter("4");
    let made: Vec<Point2> = circles(&h)
        .iter()
        .filter(|c| c.radius == 3.0)
        .map(|c| c.center)
        .collect();
    for expected in [p(40.0, 0.0), p(0.0, 40.0), p(-40.0, 0.0), p(0.0, -40.0)] {
        assert!(made.iter().any(|c| near(*c, expected)), "{made:?}");
    }
    holds(&h);

    // Any selection along a polyline, unconstrained; the path cannot be
    // one of the objects.
    let mut h = Harness::new();
    h.enter_all(&["RECTANG", "-60,-60", "-50,-55"]);
    h.enter_all(&["PLINE", "0,0", "100,0", "100,100", ""]);
    h.click_world(p(-60.0, -58.0));
    h.enter("ARRAYPATH");
    h.click_world(p(-60.0, -58.0));
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("The path cannot be one of the objects arrayed.")
    );
    h.click_world(p(50.0, 0.0));
    h.enter("3");
    assert_eq!(h.entities().len(), 4);
    assert!(h.session.drawing().constraints().is_empty());
}

#[test]
fn arraygrid_fills_a_boundary_but_its_islands() {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", "100,60", "0,60", "C"]);
    h.enter_all(&["CIRCLE", "10,10", "3"]);
    h.click_world(p(13.0, 10.0));
    h.enter("ARRAYGRID");
    assert_eq!(h.prompt(), "Pick an internal point of the boundary:");
    h.enter("50,30");
    assert_eq!(h.prompt(), "Specify the distance between columns:");
    h.enter("20");
    assert_eq!(h.prompt(), "Specify the distance between rows <20>:");
    h.enter("");
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("14 copies made inside the boundary.")
    );
    assert_eq!(circles(&h).len(), 15);
    h.undo();
    assert_eq!(circles(&h).len(), 1);

    // An island in the middle keeps the grid out.
    h.enter_all(&["CIRCLE", "50,30", "15"]);
    h.click_world(p(13.0, 10.0));
    h.enter_all(&["ARRAYGRID", "80,50", "20", "20"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("13 copies made inside the boundary.")
    );

    // No boundary round the point.
    let mut h = Harness::new();
    h.enter_all(&["CIRCLE", "10,10", "3"]);
    h.click_world(p(13.0, 10.0));
    h.enter_all(&["ARRAYGRID", "500,500"]);
    assert_eq!(
        h.log().last().map(String::as_str),
        Some("No closed boundary round that point.")
    );
}

/// Each placement by command and by the intent IR makes the same drawing:
/// the entities and the constraints.
#[test]
fn the_same_placement_by_command_and_by_intent_is_the_same_drawing() {
    // Two lines at a corner (1, 2), a circle (3), a line below (4).
    let setup: &[&[&str]] = &[
        &["LINE", "0,0", "100,0", ""],
        &["LINE", "0,0", "0,100", ""],
        &["CIRCLE", "60,40", "10"],
        &["LINE", "0,-30", "100,-30", ""],
    ];
    type Typed = fn(&mut Harness);
    let cases: [(Typed, &str); 6] = [
        (
            |h| {
                h.enter_all(&["CIRCLE", "T"]);
                h.click_world(p(50.0, 0.0));
                h.click_world(p(0.0, 50.0));
                h.enter("10");
            },
            r#"{"op": "tangent_circle", "first": {"id": 1, "near": [50, 0]}, "second": {"id": 2, "near": [0, 50]}, "radius": 10}"#,
        ),
        (
            |h| {
                h.enter_all(&["CIRCLE", "TTT"]);
                h.click_world(p(30.0, 0.0));
                h.click_world(p(0.0, 30.0));
                h.click_world(p(50.0, 40.0));
            },
            r#"{"op": "tangent_circle", "first": {"id": 1, "near": [30, 0]}, "second": {"id": 2, "near": [0, 30]}, "third": {"id": 3, "near": [50, 40]}}"#,
        ),
        (
            |h| {
                h.enter("TANARC");
                h.click_world(p(50.0, 0.0));
                h.click_world(p(0.0, 50.0));
                h.enter("15");
            },
            r#"{"op": "tangent_arc", "first": {"id": 1, "near": [50, 0]}, "second": {"id": 2, "near": [0, 50]}, "radius": 15}"#,
        ),
        (
            |h| {
                h.enter("CONCENTRIC");
                h.click_world(p(70.0, 40.0));
                h.enter("20");
            },
            r#"{"op": "concentric_circle", "of": {"id": 3}, "radius": 20}"#,
        ),
        (
            |h| {
                h.click_world(p(70.0, 40.0));
                h.enter("ARRAYPATH");
                h.click_world(p(10.0, -30.0));
                h.enter("4");
            },
            r#"{"op": "path_array", "targets": [3], "path": {"id": 4, "near": [10, -30]}, "count": 4}"#,
        ),
        (
            |h| {
                h.click_world(p(70.0, 40.0));
                h.enter("ARRAYPATH");
                h.click_world(p(10.0, -30.0));
                h.enter_all(&["M", "45"]);
            },
            r#"{"op": "path_array", "targets": [3], "path": {"id": 4, "near": [10, -30]}, "spacing": 45}"#,
        ),
    ];
    let prepared = || {
        let mut h = Harness::new();
        for typed in setup {
            h.enter_all(typed);
        }
        h
    };
    for (typed, operation) in cases {
        let mut by_command = prepared();
        let before = by_command.entities().len();
        typed(&mut by_command);
        assert!(
            by_command.entities().len() > before,
            "{:?}",
            by_command.log()
        );
        holds(&by_command);
        let mut by_intent = prepared();
        let intent = parse(&format!(
            r#"{{"version": {}, "operations": [{operation}]}}"#,
            cad_intent::VERSION
        ))
        .unwrap();
        by_intent.session.run_intent(&intent).unwrap();
        assert_eq!(by_intent.entities(), by_command.entities(), "{operation}");
        assert_eq!(
            constraint_rows(&by_intent),
            constraint_rows(&by_command),
            "{operation}"
        );
    }

    // A grid inside a box, by both.
    let boxed = || {
        let mut h = Harness::new();
        h.enter_all(&["LINE", "0,0", "100,0", "100,60", "0,60", "C"]);
        h.enter_all(&["CIRCLE", "10,10", "3"]);
        h
    };
    let mut by_command = boxed();
    by_command.click_world(p(13.0, 10.0));
    by_command.enter_all(&["ARRAYGRID", "50,30", "20", "15"]);
    let mut by_intent = boxed();
    let intent = parse(&format!(
        r#"{{"version": {}, "operations": [{{"op": "grid_array", "targets": [5], "inside": [50, 30], "column_spacing": 20, "row_spacing": 15}}]}}"#,
        cad_intent::VERSION
    ))
    .unwrap();
    by_intent.session.run_intent(&intent).unwrap();
    assert!(by_command.entities().len() > 5);
    assert_eq!(by_intent.entities(), by_command.entities());
}

fn constraint_rows(h: &Harness) -> Vec<cad_document::Constraint> {
    h.session
        .drawing()
        .constraints()
        .iter()
        .map(|(_, c)| c.clone())
        .collect()
}
