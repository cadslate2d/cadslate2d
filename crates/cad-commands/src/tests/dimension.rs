//! The dimension entity through the session (#163, ADR-0020): picked by
//! its lines and its text, gripped at its points, redrawn when its style
//! changes, moved whole, refused by the curve commands by name, taken
//! apart by EXPLODE. The layout itself is tested in `cad-document`.

use cad_core::Point2;
use cad_document::{
    DimStyle, DimStyleId, DimensionEntity, Drawing, Entity, GripKind, Measure, Properties,
};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// 200 units across, its line 50 up.
fn horizontal() -> DimensionEntity {
    DimensionEntity::new(
        Measure::Linear {
            a: p(-100.0, 0.0),
            b: p(100.0, 0.0),
            rotation: 0.0,
        },
        p(0.0, 50.0),
    )
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

#[test]
fn a_dimension_is_picked_by_its_lines_and_text_and_not_the_space_between() {
    let mut h = Harness::with_entities([Entity::from(horizontal())]);
    h.click_world(p(0.0, 50.0));
    assert_eq!(h.session.selection().len(), 1, "on the dimension line");
    let kinds: Vec<GripKind> = h.session.grips().iter().map(|grip| grip.kind).collect();
    assert_eq!(
        kinds,
        [
            GripKind::Definition,
            GripKind::Definition,
            GripKind::Placement
        ]
    );
    let summary = h.session.selection_summary();
    assert_eq!(summary.kinds, [("dimension".to_owned(), 1)]);
    let row = |label: &str| {
        summary
            .geometry
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(row("Kind"), Some("Linear"));
    assert_eq!(row("Value"), Some("200.0000"));
    assert_eq!(row("Text"), Some("200.00"));
    assert_eq!(row("Style"), Some("Standard"));
    h.escape();

    h.click_world(p(0.0, 25.0));
    assert!(
        h.session.selection().is_empty(),
        "between the extension lines is nothing"
    );
    h.escape(); // the click on nothing began a window
    h.click_world(p(0.0, 52.0));
    assert_eq!(h.session.selection().len(), 1, "on the text");
    h.escape();
    h.click_world(p(-100.0, 20.0));
    assert_eq!(h.session.selection().len(), 1, "on an extension line");
}

#[test]
fn changing_the_style_redraws_every_dimension_in_one_step() {
    let radius = DimensionEntity::new(
        Measure::Radius {
            center: p(0.0, -200.0),
            radius: 50.0,
        },
        p(20.0, -200.0),
    );
    let mut h = Harness::with_entities([Entity::from(horizontal()), Entity::from(radius)]);
    let shown = h.session.drawing().shown_revision();
    let height = |h: &Harness| {
        h.entities()
            .iter()
            .map(|entity| h.session.drawing().bounds_of(entity).max().y)
            .collect::<Vec<_>>()
    };
    let before = height(&h);
    h.enter_all(&["DIMSTYLE", "H", "10", ""]);
    assert_ne!(
        h.session.drawing().shown_revision(),
        shown,
        "the committed geometry is rebuilt"
    );
    let after = height(&h);
    assert!(
        after[0] > before[0] && after[1] > before[1],
        "{before:?} → {after:?}"
    );
    // The dimension entities themselves did not change: one row did.
    assert_eq!(dimensions(&h).len(), 2);
    assert_eq!(dimensions(&h)[0], horizontal());
    h.undo();
    assert_eq!(height(&h), before);
    // Picking finds the dimension where it is drawn now.
    h.enter_all(&["DIMSTYLE", "H", "20", ""]);
    h.click_world(p(0.0, 50.0 + 0.625 + 10.0));
    assert_eq!(h.session.selection().len(), 1, "on the taller text");
}

#[test]
fn a_style_a_dimension_is_drawn_by_cannot_be_deleted() {
    let drawing = Drawing::from_tables(
        [],
        [],
        [],
        [DimStyle::named("Arch")],
        [],
        [(
            Entity::from(horizontal().in_style(DimStyleId::from_raw(1))),
            Properties::default(),
        )],
    )
    .unwrap();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["DIMSTYLE", "D", "Arch"]);
    assert_eq!(last(&h), "Dimension style \"Arch\" is used by a dimension.");
    assert_eq!(h.session.drawing().dim_styles().len(), 2);
    // Once the dimension is gone, the style may go.
    h.enter("");
    h.click_world(p(0.0, 50.0));
    h.delete();
    h.enter_all(&["DIMSTYLE", "D", "Arch", ""]);
    assert_eq!(h.session.drawing().dim_styles().len(), 1);
}

#[test]
fn a_dimension_moves_whole_is_refused_by_the_curve_commands_and_comes_apart() {
    let mut h = Harness::with_entities([Entity::from(horizontal())]);
    h.select_box(p(-150.0, -10.0), p(150.0, 60.0));
    assert_eq!(h.session.selection().len(), 1);
    h.enter_all(&["M", "0,0", "10,20"]);
    let moved = &dimensions(&h)[0];
    assert_eq!(
        moved.measure,
        Measure::Linear {
            a: p(-90.0, 20.0),
            b: p(110.0, 20.0),
            rotation: 0.0
        }
    );
    assert_eq!(moved.line_point, p(10.0, 70.0));

    h.enter_all(&["O", "5"]);
    h.click_world(p(10.0, 70.0));
    h.click_world(p(10.0, 90.0));
    assert_eq!(last(&h), "A dimension cannot be offset.");
    h.escape();

    h.select_box(p(-150.0, 0.0), p(150.0, 90.0));
    h.enter("X");
    let kinds: Vec<&str> = h
        .entities()
        .iter()
        .map(cad_document::Entity::kind_name)
        .collect();
    assert_eq!(
        kinds,
        ["line", "line", "line", "polyline", "polyline", "text"],
        "its lines, its arrowheads, its text"
    );
    h.undo();
    assert_eq!(dimensions(&h).len(), 1);
}
