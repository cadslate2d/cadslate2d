//! DIMBASELINE and DIMCONTINUE through the session (#212): from the newest
//! dimension or one picked, per kind, with `Undo`; the whole run one undo;
//! the preview is what the point commits.

use std::f64::consts::FRAC_PI_4;

use cad_core::{Point2, Vec2};
use cad_document::{CircleEntity, DimensionEntity, Entity, LineEntity, Measure};

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

/// Each dimension's measured points and its line's height: what a chain
/// of horizontal dimensions is.
fn horizontal_chain(h: &Harness) -> Vec<(Point2, Point2, f64)> {
    dimensions(h)
        .into_iter()
        .map(|dimension| match dimension.measure {
            Measure::Linear { a, b, rotation } => {
                assert_eq!(rotation, 0.0);
                (a, b, dimension.line_point.y)
            }
            other => panic!("a linear dimension, not {other:?}"),
        })
        .collect()
}

fn previewed(h: &Harness) -> Option<DimensionEntity> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Dimension(dimension)) => Some(dimension),
        _ => None,
    })
}

/// A horizontal dimension from (0, 0) to (10, 0), its line at y = 10.
fn first() -> DimensionEntity {
    DimensionEntity::new(
        Measure::Linear {
            a: p(0.0, 0.0),
            b: p(10.0, 0.0),
            rotation: 0.0,
        },
        p(5.0, 10.0),
    )
}

#[test]
fn dimcontinue_runs_on_from_the_newest_dimension_along_its_line() {
    let mut h = Harness::new();
    h.enter_all(&["DLI", "0,0", "10,0", "5,10"]);
    h.enter("DCO");
    assert_eq!(
        h.prompt(),
        "Specify second extension line origin or [Select] <select>:"
    );
    h.enter("25,0");
    assert_eq!(
        h.prompt(),
        "Specify second extension line origin or [Undo/Select] <select>:"
    );
    h.enter("40,0");
    assert_eq!(
        horizontal_chain(&h),
        vec![
            (p(0.0, 0.0), p(10.0, 0.0), 10.0),
            (p(10.0, 0.0), p(25.0, 0.0), 10.0),
            (p(25.0, 0.0), p(40.0, 0.0), 10.0),
        ]
    );
    // Undo takes the last back; the next runs on from the one before.
    h.enter_all(&["U", "45,0"]);
    assert_eq!(horizontal_chain(&h)[2], (p(25.0, 0.0), p(45.0, 0.0), 10.0));
    // Enter asks for another dimension to chain from; Enter again ends.
    h.enter("");
    assert_eq!(h.prompt(), "Select continued dimension:");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    // The whole run is one undo; the first dimension stays.
    h.undo();
    assert_eq!(dimensions(&h).len(), 1);
    h.redo();
    assert_eq!(dimensions(&h).len(), 3);
}

#[test]
fn dimbaseline_stacks_from_the_first_extension_line_by_the_styles_spacing() {
    let mut h = Harness::new();
    h.enter_all(&["DLI", "0,0", "10,0", "5,10"]);
    h.enter_all(&["DBA", "25,0", "40,0", "", ""]);
    assert_eq!(
        horizontal_chain(&h),
        vec![
            (p(0.0, 0.0), p(10.0, 0.0), 10.0),
            (p(0.0, 0.0), p(25.0, 0.0), 13.75),
            (p(0.0, 0.0), p(40.0, 0.0), 17.5),
        ]
    );
    // A style spaced wider, and a dimension below the points: further down.
    h.enter_all(&["DIMSTYLE", "B", "6", ""]);
    h.enter_all(&["DLI", "0,-50", "10,-50", "5,-60"]);
    h.enter_all(&["DBA", "30,-50", "", ""]);
    assert_eq!(
        horizontal_chain(&h)[4],
        (p(0.0, -50.0), p(30.0, -50.0), -66.0)
    );
    // Each new dimension has the chained one's style and the current layer.
    let all = dimensions(&h);
    assert!(all.iter().all(|d| d.style == all[0].style));
    assert!(
        h.properties()
            .iter()
            .all(|properties| properties.layer == h.session.drawing().settings().current_layer)
    );
}

#[test]
fn aligned_and_angular_dimensions_chain_as_their_kind() {
    let aligned = DimensionEntity::new(
        Measure::Aligned {
            a: p(0.0, 0.0),
            b: p(30.0, 40.0),
        },
        p(15.0, 20.0) + Vec2::new(-4.0, 3.0),
    );
    let mut h = Harness::with_entities([aligned.into()]);
    h.enter_all(&["DCO", "60,80", "", ""]);
    let all = dimensions(&h);
    assert_eq!(
        all[1].measure,
        Measure::Aligned {
            a: p(30.0, 40.0),
            b: p(60.0, 80.0)
        }
    );
    assert_eq!(all[1].value(), 50.0);
    assert!(
        all[1]
            .line_point
            .distance_to(p(30.0, 40.0) + Vec2::new(-4.0, 3.0))
            < 1e-9
    );

    // A quarter from +X to +Y about the origin, its arc 20 out.
    let quarter = DimensionEntity::new(
        Measure::Angular {
            vertex: p(0.0, 0.0),
            a: p(50.0, 0.0),
            b: p(0.0, 50.0),
        },
        Point2::ORIGIN + Vec2::from_polar(20.0, FRAC_PI_4),
    );
    let mut h = Harness::with_entities([quarter.into()]);
    h.enter_all(&["DBA", "-50,0", "", ""]);
    let all = dimensions(&h);
    assert_eq!(
        all[1].measure,
        Measure::Angular {
            vertex: p(0.0, 0.0),
            a: p(50.0, 0.0),
            b: p(-50.0, 0.0)
        }
    );
    assert!((all[1].value() - 180.0).abs() < 1e-9);
    assert!((all[1].line_point.distance_to(Point2::ORIGIN) - 23.75).abs() < 1e-9);
    h.enter_all(&["DCO", "0,-50", "", ""]);
    assert!((dimensions(&h)[2].value() - 90.0).abs() < 1e-9);
}

#[test]
fn select_chains_from_the_extension_line_nearer_the_pick() {
    let mut h = Harness::with_entities([first().into()]);
    h.enter_all(&["DCO", ""]);
    assert_eq!(h.prompt(), "Select continued dimension:");
    // On the first extension line: the chain runs back from (0, 0).
    h.click_world(p(0.0, 6.0));
    assert_eq!(
        h.prompt(),
        "Specify second extension line origin or [Select] <select>:"
    );
    h.enter_all(&["-15,0", "", ""]);
    assert_eq!(horizontal_chain(&h)[1], (p(0.0, 0.0), p(-15.0, 0.0), 10.0));
}

#[test]
fn what_cannot_be_chained_is_refused() {
    let radius = DimensionEntity::new(
        Measure::Radius {
            center: p(100.0, 0.0),
            radius: 20.0,
        },
        p(130.0, 0.0),
    );
    // The newest is a radius: the command asks for one to chain from.
    let mut h = Harness::with_entities([
        first().into(),
        LineEntity::new(p(0.0, -40.0), p(50.0, -40.0)).into(),
        CircleEntity::new(p(100.0, 0.0), 20.0).into(),
        radius.into(),
    ]);
    h.enter("DCO");
    assert_eq!(
        last(&h),
        "A radius dimension cannot be chained: pick a linear, aligned, or angular one."
    );
    assert_eq!(h.prompt(), "Select continued dimension:");
    h.click_world(p(25.0, -40.0));
    assert_eq!(last(&h), "A line is not a dimension.");
    h.click_world(p(0.0, 6.0));
    // A point on the extension line it chains from measures nothing.
    h.enter("0,30");
    assert_eq!(
        last(&h),
        "The point must be off the extension line it chains from."
    );
    assert_eq!(dimensions(&h).len(), 2);
    h.escape();

    // Nothing to chain from: the command starts by asking.
    let mut h = Harness::new();
    h.enter("DBA");
    assert_eq!(h.prompt(), "Select base dimension:");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn the_preview_is_what_the_point_commits() {
    let mut h = Harness::with_entities([first().into()]);
    h.enter("DBA");
    h.move_to_world(p(32.0, 7.0));
    let shown = previewed(&h).expect("a dimension previewed");
    let revision = h.session.drawing().revision();
    h.move_to_world(p(33.0, 7.0));
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "a preview changes nothing"
    );
    h.enter("32,7");
    assert_eq!(dimensions(&h)[1], shown);
}
