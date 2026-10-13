//! Viewports (#192): MVIEW on a layout, the viewport as an entity of the
//! paper — picked, snapped to, gripped, moved, summarised — and refused
//! anywhere but on a layout's paper.

use cad_core::Point2;
use cad_document::{Drawing, Entity, Layout, LayoutId, TransactionError, ViewportEntity};

use crate::Dirty;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// A model with a 1000 by 500 rectangle, and a layout the session is on.
fn on_a_layout() -> (Harness, LayoutId) {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["REC", "0,0", "1000,500"]);
    let dirty = h.session.set_space(sheet);
    assert!(dirty.contains(Dirty::SCENE | Dirty::UI));
    assert_eq!(h.session.space(), sheet);
    (h, sheet)
}

fn viewports(h: &Harness, space: LayoutId) -> Vec<ViewportEntity> {
    h.session
        .drawing()
        .shown_in(space)
        .filter_map(|(_, entity)| match entity {
            Entity::Viewport(viewport) => Some(*viewport),
            _ => None,
        })
        .collect()
}

#[test]
fn mview_on_the_model_is_refused() {
    let mut h = Harness::new();
    h.enter("MVIEW");
    assert_eq!(
        last(&h),
        "MVIEW makes a viewport on a layout's paper: go to a layout first."
    );
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn mview_by_two_corners_shows_the_models_extents_at_the_scale_that_fits() {
    let (mut h, sheet) = on_a_layout();
    h.enter("MV");
    assert_eq!(h.prompt(), "Specify corner of viewport or [Fit]:");
    h.enter("10,10");
    assert_eq!(h.prompt(), "Specify opposite corner:");
    h.enter("210,110");
    assert_eq!(last(&h), "Viewport at 1:5 of the model.");
    let made = viewports(&h, sheet);
    assert_eq!(made.len(), 1);
    let viewport = made[0];
    assert_eq!(viewport.center, p(110.0, 60.0));
    assert_eq!((viewport.width, viewport.height), (200.0, 100.0));
    assert_eq!(viewport.view_center, p(500.0, 250.0));
    assert!((viewport.scale - 0.2).abs() < 1e-12);
    assert!(viewport.on);
    // It is the layout's, not the model's; one undo takes it away.
    assert!(viewports(&h, LayoutId::MODEL).is_empty());
    assert_eq!(
        h.session.drawing().shown().count(),
        1,
        "the rectangle alone"
    );
    h.undo();
    assert!(viewports(&h, sheet).is_empty());
    h.redo();
    assert_eq!(viewports(&h, sheet).len(), 1);
}

#[test]
fn mview_fit_fills_the_printable_area_and_a_flat_window_is_refused() {
    let (mut h, sheet) = on_a_layout();
    h.enter_all(&["MVIEW", "F"]);
    let viewport = viewports(&h, sheet)[0];
    // A layout made here: an A3 landscape sheet with the writer's margins.
    assert_eq!(viewport.center, p(210.0, 148.5));
    assert_eq!((viewport.width, viewport.height), (405.0, 257.0));
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["MVIEW", "0,0", "0,50"]);
    assert_eq!(last(&h), "The viewport has no area.");
    assert_eq!(h.prompt(), "Specify opposite corner:");
    h.escape();
}

#[test]
fn a_viewport_is_picked_by_its_frame_snapped_at_its_corners_and_summarised() {
    let (mut h, sheet) = on_a_layout();
    h.enter_all(&["MVIEW", "10,10", "210,110"]);
    // Picked on its frame, not inside it.
    h.click_world(p(110.0, 110.0));
    assert_eq!(h.session.selection().len(), 1);
    let summary = h.session.selection_summary();
    assert_eq!(summary.headline(), "1 viewport");
    assert_eq!(
        summary.geometry,
        vec![
            ("Centre".to_owned(), "110.0000, 60.0000".to_owned()),
            ("Width".to_owned(), "200.0000".to_owned()),
            ("Height".to_owned(), "100.0000".to_owned()),
            ("View centre".to_owned(), "500.0000, 250.0000".to_owned()),
            ("Scale".to_owned(), "1:5.0000".to_owned()),
            ("On".to_owned(), "yes".to_owned()),
        ]
    );
    h.escape();
    h.click_world(p(110.0, 60.0));
    assert_eq!(h.session.selection().len(), 0, "the inside is the model's");
    // Its corners, edge midpoints, and centre snap.
    h.enter("L");
    h.move_to_world(p(208.0, 108.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.point),
        Some(p(210.0, 110.0))
    );
    h.move_to_world(p(110.0, 12.0));
    assert_eq!(
        h.session.snap().map(|snap| snap.point),
        Some(p(110.0, 10.0))
    );
    h.escape();
    let _ = sheet;
}

#[test]
fn a_corner_grip_resizes_and_the_centre_grip_moves_a_viewport() {
    let (mut h, sheet) = on_a_layout();
    h.enter_all(&["MVIEW", "10,10", "210,110"]);
    h.click_world(p(110.0, 110.0));
    let grips = h.session.grips();
    assert_eq!(grips.len(), 5, "four corners and the centre");
    assert_eq!(grips[4].position, p(110.0, 60.0));
    // The top-right corner dragged out: the frame grows about the bottom left.
    h.click_world(p(210.0, 110.0));
    h.click_world(p(310.0, 160.0));
    let viewport = viewports(&h, sheet)[0];
    assert_eq!((viewport.width, viewport.height), (300.0, 150.0));
    assert_eq!(viewport.center, p(160.0, 85.0));
    assert!(
        (viewport.scale - 0.2).abs() < 1e-12,
        "the view is untouched"
    );
    // The centre moved.
    h.click_world(p(160.0, 85.0));
    h.click_world(p(200.0, 100.0));
    let viewport = viewports(&h, sheet)[0];
    assert_eq!(viewport.center, p(200.0, 100.0));
    assert_eq!((viewport.width, viewport.height), (300.0, 150.0));
    h.escape();
    // MOVE, like any entity; ROTATE keeps it square to the paper.
    h.click_world(p(50.0, 100.0));
    h.enter_all(&["M", "0,0", "10,0"]);
    assert_eq!(viewports(&h, sheet)[0].center, p(210.0, 100.0));
    h.click_world(p(60.0, 100.0));
    h.enter_all(&["RO", "0,0", "90"]);
    let turned = viewports(&h, sheet)[0];
    assert_eq!((turned.width, turned.height), (300.0, 150.0));
    assert!((turned.center.x - -100.0).abs() < 1e-9 && (turned.center.y - 210.0).abs() < 1e-9);
}

#[test]
fn a_viewport_cannot_be_put_in_the_model_nor_in_a_block() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
    let window = ViewportEntity::new(p(0.0, 0.0), 10.0, 10.0, p(0.0, 0.0), 1.0);
    let id = tx.insert_in(sheet, window).unwrap();
    tx.commit();
    let mut tx = drawing.transaction();
    assert_eq!(
        tx.set_space(id, LayoutId::MODEL),
        Err(TransactionError::ViewportOffPaper)
    );
    assert_eq!(
        tx.insert(window).err(),
        Some(TransactionError::ViewportOffPaper)
    );
    let flat = ViewportEntity::new(p(0.0, 0.0), 0.0, 10.0, p(0.0, 0.0), 1.0);
    assert!(matches!(
        tx.insert_in(sheet, flat),
        Err(TransactionError::BadViewport(_))
    ));
    tx.commit();

    let (mut h, sheet) = on_a_layout();
    h.enter_all(&["MVIEW", "10,10", "210,110"]);
    // BLOCK on the viewport is refused: a viewport is not a block's.
    h.click_world(p(110.0, 110.0));
    h.enter_all(&["BLOCK", "WINDOW", "0,0"]);
    assert!(
        last(&h).contains("viewport") || last(&h).contains("paper"),
        "{}",
        last(&h)
    );
    assert_eq!(viewports(&h, sheet).len(), 1);
}
