//! DIMLINEAR and DIMALIGNED through the session (#164): each prompt,
//! option, and refusal, one undo each; the preview is what the click
//! commits.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, DimensionEntity, Entity, LineEntity, Measure};

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

#[test]
fn dimlinear_is_horizontal_or_vertical_by_where_the_line_is_placed() {
    let mut h = Harness::new();
    h.enter("DLI");
    assert_eq!(
        h.prompt(),
        "Specify first extension line origin <select object>:"
    );
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify second extension line origin:");
    h.enter("30,40");
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Horizontal/Vertical/Rotated/Text/Angle]:"
    );
    h.enter("15,60");
    assert_eq!(h.prompt(), "Command:");
    let all = dimensions(&h);
    let [dimension] = all.as_slice() else {
        panic!("one dimension");
    };
    assert_eq!(
        dimension.measure,
        Measure::Linear {
            a: p(0.0, 0.0),
            b: p(30.0, 40.0),
            rotation: 0.0
        }
    );
    assert_eq!(dimension.line_point, p(15.0, 60.0));
    assert_eq!(dimension.value(), 30.0);
    assert_eq!(dimension.style, h.session.drawing().current_dim_style());
    assert_eq!(
        h.properties()[0].layer,
        h.session.drawing().settings().current_layer
    );

    // Beside the points: vertical.
    h.enter_all(&["DLI", "0,0", "30,40", "-20,20"]);
    let dimension = &dimensions(&h)[1];
    assert_eq!(dimension.value(), 40.0);
    let Measure::Linear { rotation, .. } = dimension.measure else {
        panic!("linear");
    };
    assert_eq!(rotation, FRAC_PI_2);
    // Beyond an end, but further above than beyond: still horizontal.
    h.enter_all(&["DLI", "0,0", "100,0", "110,50"]);
    assert_eq!(dimensions(&h)[2].value(), 100.0);
    h.enter_all(&["DLI", "0,0", "100,0", "150,10"]);
    assert!(
        dimensions(&h)[3].value() < 1e-9,
        "beside the end: vertical, of no rise"
    );

    // One undo each.
    h.undo();
    assert_eq!(dimensions(&h).len(), 3);
    h.undo();
    h.undo();
    h.undo();
    assert!(dimensions(&h).is_empty());
}

#[test]
fn the_orientation_text_and_angle_options() {
    let mut h = Harness::new();
    h.enter_all(&["DLI", "0,0", "30,40", "V", "100,20"]);
    let dimension = &dimensions(&h)[0];
    assert_eq!(dimension.value(), 40.0, "Vertical, though placed above");
    h.enter_all(&["DLI", "0,0", "30,40", "H", "-50,-5"]);
    assert_eq!(
        dimensions(&h)[1].value(),
        30.0,
        "Horizontal, though placed beside"
    );

    h.enter_all(&["DLI", "0,0", "30,40", "R"]);
    assert_eq!(h.prompt(), "Specify angle of dimension line <0>:");
    h.enter("30");
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Horizontal/Vertical/Rotated/Text/Angle]:"
    );
    h.enter("0,30");
    let dimension = &dimensions(&h)[2];
    let along = (30f64).to_radians();
    let expected = 30.0 * along.cos() + 40.0 * along.sin();
    assert!((dimension.value() - expected).abs() < 1e-9);
    // Enter at the angle is horizontal.
    h.enter_all(&["DLI", "0,0", "30,40", "R", "", "-20,20"]);
    assert_eq!(dimensions(&h)[3].value(), 30.0);

    h.enter_all(&["DLI", "0,0", "30,40", "T"]);
    assert_eq!(h.prompt(), "Enter dimension text <30.00>:");
    h.enter("3 x 10 TYP");
    h.enter("A");
    assert_eq!(h.prompt(), "Specify angle of dimension text <0>:");
    h.enter("45");
    h.enter("15,60");
    let dimension = &dimensions(&h)[4];
    assert_eq!(dimension.text_override.as_deref(), Some("3 x 10 TYP"));
    assert!((dimension.text_angle.unwrap() - 45f64.to_radians()).abs() < 1e-12);
    let style = h.session.drawing().tables().dim_style(dimension.style);
    let text = dimension.layout(style).text;
    assert_eq!(text.content, "3 x 10 TYP");
    assert!((text.rotation - 45f64.to_radians()).abs() < 1e-12);
    // Enter at the text keeps the value; Enter at the angle, the line's.
    h.enter_all(&["DLI", "0,0", "30,40", "T", "", "A", "", "15,60"]);
    let dimension = &dimensions(&h)[5];
    assert_eq!(
        (dimension.text_override.as_deref(), dimension.text_angle),
        (None, None)
    );
}

#[test]
fn enter_and_a_picked_line_or_arc_gives_its_ends() {
    let mut h = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        ArcEntity::new(p(0.0, -100.0), 50.0, 0.0, FRAC_PI_2).into(),
        CircleEntity::new(p(-200.0, 0.0), 20.0).into(),
    ]);
    h.enter_all(&["DLI", ""]);
    assert_eq!(h.prompt(), "Select object to dimension:");
    h.click_world(p(-200.0, 20.0));
    assert_eq!(
        last(&h),
        "A circle has no ends to dimension: pick a line or an arc."
    );
    assert_eq!(h.prompt(), "Select object to dimension:");
    h.click_world(p(50.0, 0.0));
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Horizontal/Vertical/Rotated/Text/Angle]:"
    );
    h.enter("50,30");
    assert_eq!(
        dimensions(&h)[0].measure,
        Measure::Linear {
            a: p(0.0, 0.0),
            b: p(100.0, 0.0),
            rotation: 0.0
        }
    );
    // An arc: its ends.
    h.enter_all(&["DAL", ""]);
    h.click_world(p(50.0 * 0.5f64.sqrt(), -100.0 + 50.0 * 0.5f64.sqrt()));
    h.enter("80,-40");
    let Measure::Aligned { a, b } = dimensions(&h)[1].measure else {
        panic!("aligned");
    };
    assert!(a.distance_to(p(50.0, -100.0)) < 1e-9 && b.distance_to(p(0.0, -50.0)) < 1e-9);
    // Enter at the object prompt ends the command.
    h.enter_all(&["DLI", "", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn dimaligned_measures_along_the_points_and_offers_no_orientation() {
    let mut h = Harness::new();
    h.enter_all(&["DAL", "0,0", "30,40"]);
    assert_eq!(
        h.prompt(),
        "Specify dimension line location or [Text/Angle]:"
    );
    h.enter("-8,6");
    let dimension = &dimensions(&h)[0];
    assert_eq!(
        dimension.measure,
        Measure::Aligned {
            a: p(0.0, 0.0),
            b: p(30.0, 40.0)
        }
    );
    assert_eq!(dimension.value(), 50.0);
    assert_eq!(dimension.line_point, p(-8.0, 6.0));
    h.undo();
    assert!(dimensions(&h).is_empty());
}

#[test]
fn the_preview_is_the_dimension_the_click_commits_and_touches_nothing() {
    let mut h = Harness::new();
    let revision = h.session.drawing().revision();
    h.enter_all(&["DLI", "0,0", "30,40"]);
    h.move_to_world(p(15.0, 60.0));
    let preview = previewed(&h).expect("the dimension at the pointer");
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(dimensions(&h).is_empty());
    // Moving the pointer beside the points turns the preview vertical.
    h.move_to_world(p(-20.0, 20.0));
    assert_eq!(previewed(&h).unwrap().value(), 40.0);
    h.move_to_world(p(15.0, 60.0));
    h.click_world(p(15.0, 60.0));
    assert_eq!(dimensions(&h), vec![preview]);
    assert_ne!(h.session.drawing().revision(), revision);
    assert!(previewed(&h).is_none(), "nothing previewed once committed");
    // At the second point, a rubber band from the first.
    h.enter_all(&["DLI", "0,0"]);
    h.move_to_world(p(10.0, 10.0));
    assert!(h.session.overlay().iter().any(|item| matches!(
        item,
        OverlayItem::PreviewSegment { start, end } if *start == p(0.0, 0.0) && *end == p(10.0, 10.0)
    )));
    h.escape();
}

#[test]
fn coincident_points_are_refused_and_the_current_style_is_taken() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "N", "Arch", "H", "5", ""]);
    h.enter_all(&["DLI", "10,10", "10,10"]);
    assert_eq!(last(&h), "The two points of a dimension must be apart.");
    assert_eq!(h.prompt(), "Specify second extension line origin:");
    h.enter_all(&["50,10", "30,30"]);
    let dimension = &dimensions(&h)[0];
    assert_eq!(
        dimension.style,
        h.session.drawing().dim_styles().find("Arch").unwrap()
    );
    let style = h.session.drawing().tables().dim_style(dimension.style);
    assert_eq!(dimension.layout(style).text.height, 5.0);
    // Escape anywhere leaves nothing behind.
    h.enter_all(&["DLI", "0,0"]);
    h.escape();
    assert_eq!(dimensions(&h).len(), 1);
    assert_eq!(h.prompt(), "Command:");
}
