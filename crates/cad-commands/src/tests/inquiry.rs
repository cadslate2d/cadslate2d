//! Inquiry (#197): DIST, ID, AREA, LIST answer in the log and change
//! nothing.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::{Point2, Vec2};
use cad_document::{
    ArcEntity, AttributeDefinitionEntity, Block, BlockReferenceEntity, CircleEntity,
    DimensionEntity, Drawing, EllipseEntity, Entity, EntityRecord, HatchEntity, LineEntity,
    MTextEntity, Measure, Path, PolylineEntity, Properties, SplineEntity, TextEntity,
};
use proptest::prelude::*;

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

#[test]
fn dist_reports_the_distance_its_deltas_and_the_angle_and_changes_nothing() {
    let mut h = Harness::with_entities([line(0.0, 0.0, 100.0, 0.0)]);
    let revision = h.session.drawing().revision();
    h.enter("DIST");
    assert_eq!(h.prompt(), "Specify first point:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify second point:");
    h.enter("100,100");
    assert_eq!(
        last(&h),
        "Distance = 141.4214, Delta X = 100.0000, Delta Y = 100.0000, Angle = 45.0000°"
    );
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().revision(), revision);
    // Relative to the first point, and the unit's suffix once there is one.
    h.enter_all(&["UNITS", "mm", "DI", "10,10", "@-30,-40"]);
    assert_eq!(
        last(&h),
        "Distance = 50.0000 mm, Delta X = -30.0000 mm, Delta Y = -40.0000 mm, Angle = 233.1301°"
    );
}

#[test]
fn id_reports_a_point() {
    let mut h = Harness::new();
    h.enter("ID");
    assert_eq!(h.prompt(), "Specify point:");
    h.enter("12.5,-3");
    assert_eq!(last(&h), "X = 12.5000, Y = -3.0000");
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["UNITS", "m", "ID", "1,2"]);
    assert_eq!(last(&h), "X = 1.0000 m, Y = 2.0000 m");
}

#[test]
fn area_by_points_closes_the_polygon_on_enter_and_needs_three() {
    let mut h = Harness::new();
    h.enter("AREA");
    assert_eq!(h.prompt(), "Specify first corner point or [Object]:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify next point:");
    h.enter_all(&["100,0", "100,50", ""]);
    assert_eq!(last(&h), "Area = 2500.0000, Perimeter = 261.8034");
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["AA", "0,0", "10,0", ""]);
    assert_eq!(last(&h), "An area needs at least three points.");
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["UNITS", "mm", "AREA", "0,0", "10,0", "10,10", "0,10", ""]);
    assert_eq!(last(&h), "Area = 100.0000 mm², Perimeter = 40.0000 mm");
}

#[test]
fn area_object_measures_each_closed_kind_and_refuses_the_rest() {
    let mut h = Harness::with_entities([
        CircleEntity::new(p(0.0, 0.0), 10.0).into(),
        PolylineEntity::with_bulges(
            vec![p(100.0, 0.0), p(110.0, 0.0), p(110.0, 10.0), p(100.0, 10.0)],
            vec![1.0, 0.0, 0.0, 0.0],
            true,
        )
        .into(),
        EllipseEntity::new(p(200.0, 0.0), Vec2::new(40.0, 0.0), 0.5).into(),
        HatchEntity::solid(vec![Path {
            vertices: vec![p(300.0, 0.0), p(360.0, 0.0), p(360.0, 60.0), p(300.0, 60.0)],
            bulges: vec![],
            closed: true,
        }])
        .into(),
        PolylineEntity::new(vec![p(400.0, 0.0), p(450.0, 0.0), p(450.0, 50.0)], false).into(),
        line(500.0, 0.0, 600.0, 0.0),
    ]);
    h.enter_all(&["ZOOM", "E"]);
    let revision = h.session.drawing().revision();
    let measure = |h: &mut Harness, at: Point2| {
        h.enter_all(&["AREA", "O"]);
        assert_eq!(h.prompt(), "Select object:");
        h.click_world(at);
        last(h)
    };
    assert_eq!(
        measure(&mut h, p(10.0, 0.0)),
        "Area = 314.1593, Perimeter = 62.8319"
    );
    // The square with its bottom side bowed out by a half-disc.
    let bowed = 100.0 + PI * 25.0 / 2.0;
    assert_eq!(
        measure(&mut h, p(100.0, 5.0)),
        format!("Area = {bowed:.4}, Perimeter = {:.4}", 30.0 + PI * 5.0)
    );
    // The ellipse's area exactly; its perimeter as its chords, within a
    // hundredth of Ramanujan's.
    let report = measure(&mut h, p(240.0, 0.0));
    assert!(
        report.starts_with("Area = 2513.2741, Perimeter = 193."),
        "{report}"
    );
    assert_eq!(
        measure(&mut h, p(300.0, 30.0)),
        "Area = 3600.0000, Perimeter = 240.0000"
    );
    assert_eq!(measure(&mut h, p(425.0, 0.0)), "That object is not closed.");
    assert_eq!(h.prompt(), "Select object:");
    h.escape();
    assert_eq!(
        measure(&mut h, p(550.0, 0.0)),
        "An area needs a closed polyline, a circle, an ellipse, or a hatch."
    );
    h.escape();
    assert_eq!(h.session.drawing().revision(), revision);
}

proptest! {
    /// #197: a polygon's area by its corners is its closed polyline's.
    #[test]
    fn a_polygons_area_by_points_is_its_polylines(
        corners in prop::collection::vec((-100.0..100.0_f64, -100.0..100.0_f64), 3..8),
    ) {
        let points: Vec<Point2> = corners.iter().map(|(x, y)| p(*x, *y)).collect();
        let mut h = Harness::with_entities([PolylineEntity::new(points.clone(), true).into()]);
        let typed: Vec<String> = points.iter().map(|c| format!("{},{}", c.x, c.y)).collect();
        h.enter("AREA");
        for corner in &typed {
            h.enter(corner);
        }
        h.enter("");
        let by_points = last(&h);
        h.enter_all(&["ZOOM", "E", "AREA", "O"]);
        // The polyline's first corner, on its first edge.
        h.click_world(points[0]);
        let by_object = last(&h);
        prop_assert_eq!(by_points, by_object);
    }
}

/// One of every kind the model can hold, on layer `0`.
fn everything() -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let part = tx
        .define_block(Block::new(
            "Part",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                line(0.0, 0.0, 10.0, 10.0),
                Properties::default(),
            )],
        ))
        .unwrap();
    for entity in [
        line(0.0, 0.0, 100.0, 0.0),
        CircleEntity::new(p(150.0, 50.0), 30.0).into(),
        ArcEntity::new(p(250.0, 50.0), 30.0, 0.0, PI).into(),
        EllipseEntity::new(p(350.0, 50.0), Vec2::new(40.0, 0.0), 0.5).into(),
        SplineEntity::through(&[p(0.0, 100.0), p(50.0, 150.0), p(100.0, 100.0)])
            .unwrap()
            .into(),
        PolylineEntity::with_bulges(
            vec![p(150.0, 100.0), p(250.0, 100.0), p(250.0, 150.0)],
            vec![0.0, 1.0, 0.0],
            true,
        )
        .into(),
        TextEntity::new(p(300.0, 120.0), 10.0, 0.0, "Plot").into(),
        MTextEntity::new(p(300.0, 160.0), 80.0, 8.0, 0.0, "Two\nlines").into(),
        AttributeDefinitionEntity::new(
            "ROOM",
            "Room",
            TextEntity::new(p(0.0, 300.0), 5.0, 0.0, "101"),
        )
        .into(),
        BlockReferenceEntity::new(part, p(0.0, 200.0), 2.0, FRAC_PI_2).into(),
        DimensionEntity::new(
            Measure::Linear {
                a: p(100.0, 200.0),
                b: p(200.0, 200.0),
                rotation: 0.0,
            },
            p(150.0, 230.0),
        )
        .into(),
        HatchEntity::solid(vec![Path {
            vertices: vec![
                p(300.0, 200.0),
                p(360.0, 200.0),
                p(360.0, 260.0),
                p(300.0, 260.0),
            ],
            bulges: vec![],
            closed: true,
        }])
        .into(),
    ] {
        tx.insert(entity).unwrap();
    }
    tx.commit();
    drawing
}

#[test]
fn list_names_every_kind_with_its_properties_its_space_and_what_defines_it() {
    let mut h = Harness::with_drawing(everything());
    let revision = h.session.drawing().revision();
    let before = h.log().len();
    h.enter("LIST");
    assert!(
        h.prompt().starts_with("Select objects or ["),
        "{}",
        h.prompt()
    );
    h.enter_all(&["ALL", ""]);
    assert_eq!(h.prompt(), "Command:");
    // After the echoes of what was typed and the prompts.
    let listed: Vec<String> = h.log()[before..].to_vec();
    assert!(
        listed.contains(&"12 found, 12 total".to_owned()),
        "{listed:?}"
    );
    let start = listed
        .iter()
        .position(|line| line.starts_with("LINE  "))
        .expect("the first object");
    let listed: Vec<&str> = listed[start..].iter().map(String::as_str).collect();
    let common =
        "Layer: \"0\"  Colour: ByLayer  Linetype: ByLayer  Lineweight: ByLayer  Space: model space";
    let expected: Vec<String> = [
        "LINE  {common}",
        "  Start: 0.0000, 0.0000",
        "  End: 100.0000, 0.0000",
        "  Length: 100.0000",
        "  Angle: 0.0000°",
        "CIRCLE  {common}",
        "  Centre: 150.0000, 50.0000",
        "  Radius: 30.0000",
        "  Diameter: 60.0000",
        "  Circumference: 188.4956",
        "ARC  {common}",
        "  Centre: 250.0000, 50.0000",
        "  Radius: 30.0000",
        "  Start angle: 0.0000°",
        "  End angle: 180.0000°",
        "  Arc length: 94.2478",
        "ELLIPSE  {common}",
        "  Centre: 350.0000, 50.0000",
        "  Major axis: 80.0000",
        "  Minor axis: 40.0000",
        "  Ratio: 0.5000",
        "  Rotation: 0.0000°",
        "SPLINE  {common}",
        "  Degree: 2",
        "  Control points: 3",
        "  Fit points: 3",
        "  Closed: No",
        "  Length: 147.8824",
        "POLYLINE  {common}",
        "  Vertices: 3",
        "  Closed: Yes",
        "  Length: 290.3432",
        "TEXT  {common}",
        "  Contents: Plot",
        "  Insertion: 300.0000, 120.0000",
        "  Height: 10.0000",
        "  Rotation: 0.0000°",
        "  Justification: Left",
        "  Width: 28.5714",
        "MULTILINE TEXT  {common}",
        "  Contents: Two ⏎ lines",
        "  Insertion: 300.0000, 160.0000",
        "  Height: 8.0000",
        "  Width: 80.0000",
        "  Rotation: 0.0000°",
        "  Lines: 2",
        "ATTRIBUTE DEFINITION  {common}",
        "  Tag: ROOM",
        "  Prompt: Room",
        "  Default: 101",
        "  Insertion: 0.0000, 300.0000",
        "  Height: 5.0000",
        "BLOCK REFERENCE  {common}",
        "  Block: Part",
        "  Insertion: 0.0000, 200.0000",
        "  Scale: 2.0000",
        "  Rotation: 90.0000°",
        "  Mirrored: No",
        "DIMENSION  {common}",
        "  Kind: Linear",
        "  Value: 100.0000",
        "  Text: 100.00",
        "  Style: Standard",
        "HATCH  {common}",
        "  Pattern: SOLID",
        "  Loops: 1",
        "  Area: 3600.0000",
    ]
    .iter()
    .map(|line| line.replace("{common}", common))
    .collect();
    let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(listed, expected);
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(h.session.selection().is_empty(), "the selection is spent");
}

#[test]
fn list_on_a_layout_names_the_layout_and_a_viewport_and_lists_nothing_for_nothing() {
    let mut h = Harness::with_entities([line(0.0, 0.0, 100.0, 0.0)]);
    h.enter_all(&["LAYOUT", "N", "Sheet one", "MVIEW", "10,10", "210,110"]);
    let before = h.log().len();
    h.enter_all(&["LI", "ALL", ""]);
    let listed: Vec<String> = h.log()[before..].to_vec();
    let start = listed
        .iter()
        .position(|line| line.starts_with("VIEWPORT  "))
        .expect("the viewport");
    assert_eq!(
        listed[start],
        "VIEWPORT  Layer: \"0\"  Colour: ByLayer  Linetype: ByLayer  Lineweight: ByLayer  Space: layout \"Sheet one\""
    );
    assert_eq!(listed[start + 1], "  Centre: 110.0000, 60.0000");
    assert_eq!(listed.len() - start, 1 + 6, "{listed:?}");
    h.enter_all(&["LIST", ""]);
    assert_eq!(last(&h), "No objects selected.");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn inquiry_commits_nothing_so_u_undoes_the_edit_before() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    assert_eq!(h.session.drawing().shown().count(), 1);
    h.enter_all(&[
        "DIST", "0,0", "50,0", "ID", "1,1", "AREA", "0,0", "1,0", "1,1", "",
    ]);
    h.click_world(p(50.0, 0.0));
    h.enter("LIST");
    assert_eq!(h.prompt(), "Command:", "the selection was listed at once");
    h.enter("U");
    assert_eq!(
        h.session.drawing().shown().count(),
        0,
        "U after the inquiries undoes the line: {:?}",
        h.log()
    );
}
