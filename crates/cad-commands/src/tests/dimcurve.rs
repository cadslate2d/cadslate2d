//! DIMANGULAR, DIMRADIUS, and DIMDIAMETER through the session (#165): per
//! command, per pick kind, per refusal, one undo each; and, under
//! property tests, that the angle is the lines' and the radius the
//! circle's.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use cad_core::{Point2, Vec2};
use cad_document::{
    ArcEntity, CircleEntity, DimensionEntity, Entity, LineEntity, Measure, TextEntity,
};
use proptest::prelude::*;

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn dimensions(h: &Harness) -> Vec<DimensionEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .collect()
}

fn previewed(h: &Harness) -> Option<DimensionEntity> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Dimension(dimension)) => Some(dimension),
        _ => None,
    })
}

const PLACE: &str = "Specify dimension arc line location or [Text/Angle]:";

#[test]
fn dimangular_takes_an_arc_a_circle_two_lines_or_a_vertex_and_two_points() {
    let mut h = Harness::with_entities([
        ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, FRAC_PI_2).into(),
        CircleEntity::new(p(200.0, 0.0), 20.0).into(),
        LineEntity::new(p(-300.0, -100.0), p(-100.0, 100.0)).into(),
        LineEntity::new(p(-300.0, 100.0), p(-100.0, -100.0)).into(),
        TextEntity::new(p(0.0, -200.0), 10.0, 0.0, "no").into(),
    ]);
    // An arc: its centre and its ends.
    h.enter("DAN");
    assert_eq!(h.prompt(), "Select arc, circle, line <specify vertex>:");
    h.click_world(p(50.0 * 0.5f64.sqrt(), 50.0 * 0.5f64.sqrt()));
    assert_eq!(h.prompt(), PLACE);
    h.enter("60,60");
    let dimension = &dimensions(&h)[0];
    let Measure::Angular { vertex, a, b } = dimension.measure else {
        panic!("angular");
    };
    assert_eq!(vertex, p(0.0, 0.0));
    assert!(a.distance_to(p(50.0, 0.0)) < 1e-9 && b.distance_to(p(0.0, 50.0)) < 1e-9);
    assert!((dimension.value() - 90.0).abs() < 1e-9);
    assert_eq!(dimension.line_point, p(60.0, 60.0));

    // A circle: its centre, the pick, and a second point.
    h.enter("DAN");
    h.click_world(p(220.0, 0.0));
    assert_eq!(h.prompt(), "Specify second angle endpoint:");
    h.enter("200,30");
    assert_eq!(h.prompt(), PLACE);
    h.enter("215,15");
    assert!((dimensions(&h)[1].value() - 90.0).abs() < 1e-9);

    // Two lines: where they meet, and the angle the pointer is in.
    h.enter("DAN");
    h.click_world(p(-150.0, 50.0));
    assert_eq!(h.prompt(), "Select second line:");
    h.click_world(p(-150.0, -50.0));
    assert_eq!(h.prompt(), PLACE);
    h.enter("-140,0");
    let right = &dimensions(&h)[2];
    assert!((right.value() - 90.0).abs() < 1e-9);
    let Measure::Angular { vertex, .. } = right.measure else {
        panic!("angular");
    };
    assert!(
        vertex.distance_to(p(-200.0, 0.0)) < 1e-9,
        "where the lines cross"
    );

    // The pointer in another of the four angles measures that one.
    h.enter_all(&["DAN", ""]);
    assert_eq!(h.prompt(), "Specify angle vertex:");
    h.enter("0,-100");
    assert_eq!(h.prompt(), "Specify first angle endpoint:");
    h.enter("100,-100");
    assert_eq!(h.prompt(), "Specify second angle endpoint:");
    h.enter("50,-13.397459621556");
    assert_eq!(h.prompt(), PLACE);
    h.enter("60,-80");
    assert!((dimensions(&h)[3].value() - 60.0).abs() < 1e-9);
    h.enter_all(&[
        "DAN",
        "",
        "0,-100",
        "100,-100",
        "50,-13.397459621556",
        "-60,-80",
    ]);
    assert!(
        (dimensions(&h)[4].value() - 300.0).abs() < 1e-9,
        "the reflex side"
    );

    // Refusals: a point on the vertex, a text, a second pick that is not
    // a line, parallel lines.
    h.enter_all(&["DAN", "", "0,0", "0,0"]);
    assert_eq!(
        last(&h),
        "The points of an angle must be away from its vertex."
    );
    assert_eq!(h.prompt(), "Specify first angle endpoint:");
    h.escape();
    h.enter("DAN");
    h.click_world(p(0.0, -195.0));
    assert_eq!(
        last(&h),
        "A text has no angle to dimension: pick an arc, a circle, or a line."
    );
    h.click_world(p(-150.0, 50.0));
    h.click_world(p(220.0, 0.0));
    assert_eq!(last(&h), "Pick a second line.");
    h.escape();
    let mut parallel = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        LineEntity::new(p(0.0, 50.0), p(100.0, 50.0)).into(),
    ]);
    parallel.enter("DAN");
    parallel.click_world(p(50.0, 0.0));
    parallel.click_world(p(50.0, 50.0));
    assert_eq!(
        last(&parallel),
        "The lines are parallel: they make no angle."
    );
    assert_eq!(parallel.prompt(), "Select second line:");

    // One undo each.
    assert_eq!(dimensions(&h).len(), 5);
    h.undo();
    assert_eq!(dimensions(&h).len(), 4);
}

#[test]
fn dimradius_and_dimdiameter_take_an_arc_or_a_circle() {
    let mut h = Harness::with_entities([
        CircleEntity::new(p(0.0, 0.0), 40.0).into(),
        ArcEntity::new(p(200.0, 0.0), 30.0, 0.0, PI).into(),
        LineEntity::new(p(-200.0, 0.0), p(-100.0, 0.0)).into(),
    ]);
    h.enter("DRA");
    assert_eq!(h.prompt(), "Select arc or circle:");
    h.click_world(p(-150.0, 0.0));
    assert_eq!(
        last(&h),
        "A line has no radius to dimension: pick an arc or a circle."
    );
    h.click_world(p(40.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Text/Angle]:"
    );
    h.enter("20,0");
    let radius = &dimensions(&h)[0];
    assert_eq!(
        radius.measure,
        Measure::Radius {
            center: p(0.0, 0.0),
            radius: 40.0
        }
    );
    assert_eq!(radius.line_point, p(20.0, 0.0));
    let style = h.session.drawing().tables().dim_style(radius.style).clone();
    assert_eq!(radius.content(&style), "R40.00");

    h.enter("DDI");
    h.click_world(p(200.0, 30.0));
    h.enter("200,60");
    let diameter = &dimensions(&h)[1];
    assert_eq!(
        diameter.measure,
        Measure::Diameter {
            center: p(200.0, 0.0),
            radius: 30.0
        }
    );
    assert_eq!(diameter.value(), 60.0);
    assert_eq!(diameter.content(&style), "Ø60.00");

    h.undo();
    assert_eq!(dimensions(&h).len(), 1);
    h.undo();
    assert!(dimensions(&h).is_empty());
    // Enter at the object ends the command.
    h.enter_all(&["DDI", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn text_and_angle_options_and_the_preview_is_what_is_committed() {
    let mut h = Harness::with_entities([CircleEntity::new(p(0.0, 0.0), 40.0).into()]);
    let revision = h.session.drawing().revision();
    h.enter("DRA");
    h.click_world(p(0.0, 40.0));
    h.enter("T");
    assert_eq!(h.prompt(), "Enter dimension text <R40.00>:");
    h.enter("R40 TYP");
    h.enter("A");
    assert_eq!(h.prompt(), "Specify angle of dimension text <0>:");
    h.enter("90");
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Text/Angle]:"
    );
    h.move_to_world(p(0.0, 20.0));
    let preview = previewed(&h).expect("the dimension at the pointer");
    assert_eq!(preview.text_override.as_deref(), Some("R40 TYP"));
    assert!((preview.text_angle.unwrap() - FRAC_PI_2).abs() < 1e-12);
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "nothing committed"
    );
    h.click_world(p(0.0, 20.0));
    assert_eq!(dimensions(&h), vec![preview]);
    assert!(previewed(&h).is_none());

    // The angular text default is the measured angle.
    h.enter_all(&["DAN", "", "0,-100", "100,-100", "0,0", "T"]);
    assert_eq!(h.prompt(), "Enter dimension text <90.00°>:");
    h.enter_all(&["", "60,-70"]);
    assert_eq!(dimensions(&h)[1].text_override, None);
    assert!((dimensions(&h)[1].value() - 90.0).abs() < 1e-9);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The angle DIMANGULAR measures between two lines is the angle between
    /// them, or its supplement: whichever the placement is in.
    #[test]
    fn the_angular_value_is_the_angle_between_the_lines(
        angle_a in 0.0f64..PI, turn in 0.1f64..(PI - 0.1), place_angle in 0.0f64..TAU,
    ) {
        let angle_b = angle_a + turn;
        let along = |angle: f64| Vec2::from_polar(1.0, angle);
        let line = |angle: f64| {
            Entity::from(LineEntity::new(
                Point2::ORIGIN - along(angle) * 200.0,
                Point2::ORIGIN + along(angle) * 200.0,
            ))
        };
        let mut h = Harness::with_entities([line(angle_a), line(angle_b)]);
        h.enter("DAN");
        h.click_world(Point2::ORIGIN + along(angle_a) * 100.0);
        h.click_world(Point2::ORIGIN + along(angle_b) * 100.0);
        prop_assert_eq!(h.prompt(), PLACE);
        h.click_world(Point2::ORIGIN + along(place_angle) * 60.0);
        let dimensions = dimensions(&h);
        prop_assert_eq!(dimensions.len(), 1);
        let value = dimensions[0].value();
        let between = turn.to_degrees();
        prop_assert!(
            (value - between).abs() < 1e-6 || (value - (180.0 - between)).abs() < 1e-6,
            "{value} for lines {between}° apart"
        );
        prop_assert!(value <= 180.0 + 1e-9, "never a reflex angle between lines");
    }

    /// The radius DIMRADIUS measures is the circle's; DIMDIAMETER's twice it.
    #[test]
    fn the_radius_is_the_circles(
        cx in -100.0f64..100.0, cy in -100.0f64..100.0, radius in 5.0f64..80.0,
        place_angle in 0.0f64..TAU, diameter in proptest::bool::ANY,
    ) {
        let center = p(cx, cy);
        let mut h = Harness::with_entities([CircleEntity::new(center, radius).into()]);
        h.enter(if diameter { "DDI" } else { "DRA" });
        h.click_world(center + Vec2::new(radius, 0.0));
        h.click_world(center + Vec2::from_polar(radius / 2.0, place_angle));
        let dimensions = dimensions(&h);
        prop_assert_eq!(dimensions.len(), 1);
        let expected = if diameter { 2.0 * radius } else { radius };
        prop_assert!((dimensions[0].value() - expected).abs() < 1e-9);
    }
}
