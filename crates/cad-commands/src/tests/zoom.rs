//! ZOOM and PAN (#180): the view moved by name, committing nothing.

use cad_core::{Aabb, Point2};

use crate::Dirty;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A line far from the origin, so the default view shows none of it.
fn far_line() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["L", "1000,1000", "1400,1300", ""]);
    h
}

#[test]
fn zoom_extents_fits_the_drawing_and_enter_is_extents() {
    let mut h = far_line();
    let revision = h.session.drawing().revision();
    h.enter("ZOOM");
    assert_eq!(
        h.prompt(),
        "Specify corner of window or [All/Center/Extents/Previous/Scale/Window] <Extents>:"
    );
    let dirty = h.enter("E");
    assert!(dirty.contains(Dirty::VIEW));
    assert_eq!(h.prompt(), "Command:");
    let view = h.session.view();
    assert_eq!(view.center(), p(1200.0, 1150.0));
    // 400 × 300 in 800 × 600 at a 90% fill: 1.8 px per unit.
    assert!((view.scale() - 1.8).abs() < 1e-9, "{}", view.scale());
    // Nothing committed: the same revision, and U undoes the line, not the view.
    assert_eq!(h.session.drawing().revision(), revision);
    h.undo();
    assert_eq!(h.session.drawing().len(), 0);
    assert_eq!(h.session.view().center(), p(1200.0, 1150.0));

    // Enter alone is Extents, and All is Extents too.
    let mut h = far_line();
    h.enter_all(&["Z", ""]);
    assert_eq!(h.session.view().center(), p(1200.0, 1150.0));
    let mut h = far_line();
    h.enter_all(&["Z", "A"]);
    assert_eq!(h.session.view().center(), p(1200.0, 1150.0));
}

#[test]
fn an_empty_drawing_has_nothing_to_zoom_to() {
    let mut h = Harness::new();
    let before = h.session.view().clone();
    let dirty = h.enter_all_dirty(&["ZOOM", ""]);
    assert!(!dirty.contains(Dirty::VIEW));
    assert_eq!(h.session.view(), &before);
    assert_eq!(
        h.log().last().unwrap(),
        "Nothing to zoom to: the drawing is empty."
    );
}

#[test]
fn zoom_window_frames_the_corners_typed_or_picked() {
    let mut h = far_line();
    h.enter_all(&["ZOOM", "W", "1000,1000", "1200,1100"]);
    assert_eq!(h.prompt(), "Command:");
    let view = h.session.view();
    assert_eq!(view.center(), p(1100.0, 1050.0));
    assert!((view.scale() - 3.6).abs() < 1e-9, "{}", view.scale());
    // The corners come straight too, and may be picked: a window's corner
    // is a place on the screen, which object snap does not move.
    let mut h = far_line();
    h.enter("ZOOM");
    h.move_to_world(p(1000.3, 999.7));
    assert_eq!(
        h.session.snap(),
        None,
        "a corner does not snap to the line's end"
    );
    h.click_world(p(-100.0, -100.0));
    assert_eq!(h.prompt(), "Specify opposite corner:");
    h.click_world(p(100.0, 50.0));
    assert_eq!(h.session.view().center(), p(0.0, -25.0));
    assert!((h.session.view().scale() - 3.6).abs() < 1e-9);
    // A window of no size is refused.
    let mut h = far_line();
    h.enter_all(&["ZOOM", "W", "5,5", "5,5"]);
    assert_eq!(h.prompt(), "Specify opposite corner:");
    assert_eq!(h.log().last().unwrap(), "The window has no size.");
}

#[test]
fn zoom_scale_is_relative_to_the_view_and_takes_an_x() {
    let mut h = Harness::new();
    h.enter_all(&["ZOOM", "S", "2x"]);
    assert_eq!(h.session.view().scale(), 2.0);
    assert_eq!(h.session.view().center(), Point2::ORIGIN);
    h.enter_all(&["Z", "S", "0.25"]);
    assert_eq!(h.session.view().scale(), 0.5);
    h.enter_all(&["Z", "S", "-1"]);
    assert_eq!(h.prompt(), "Enter a scale factor (2x, 0.5x):");
    assert_eq!(
        h.log().last().unwrap(),
        "The factor must be a positive number."
    );
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.view().scale(), 0.5);
}

#[test]
fn zoom_center_recentres_and_takes_a_height() {
    let mut h = far_line();
    h.enter_all(&["ZOOM", "C", "1000,1000", ""]);
    assert_eq!(h.session.view().center(), p(1000.0, 1000.0));
    assert_eq!(h.session.view().scale(), 1.0, "Enter keeps the height");
    h.enter_all(&["ZOOM", "C", "1200,1150", "300"]);
    assert_eq!(h.session.view().center(), p(1200.0, 1150.0));
    assert_eq!(h.session.view().scale(), 2.0, "600 px over 300 units");
    // The centre is a point of the drawing: it snaps.
    h.enter("ZOOM");
    h.enter("C");
    h.move_to_world(p(1000.4, 999.6));
    assert_eq!(
        h.session.snap().map(|snap| snap.point),
        Some(p(1000.0, 1000.0))
    );
    h.escape();
}

#[test]
fn zoom_previous_walks_back_through_the_views_zoom_and_pan_made() {
    let mut h = far_line();
    let start = h.session.view().clone();
    h.enter_all(&["ZOOM", "E"]);
    let fitted = h.session.view().clone();
    h.enter_all(&["ZOOM", "S", "2"]);
    h.enter_all(&["PAN", "0,0", "100,0"]);
    h.enter_all(&["ZOOM", "P"]);
    h.enter_all(&["ZOOM", "P"]);
    assert_eq!(h.session.view(), &fitted);
    h.enter_all(&["ZOOM", "P"]);
    assert_eq!(h.session.view(), &start);
    let dirty = h.enter_all_dirty(&["ZOOM", "P"]);
    assert_eq!(h.log().last().unwrap(), "No previous view.");
    assert!(!dirty.contains(Dirty::VIEW));
    // A view the wheel made is not on the stack: Previous is ZOOM's own.
    assert_eq!(h.session.view(), &start);
}

#[test]
fn pan_moves_the_drawing_by_two_points_or_a_displacement() {
    let mut h = Harness::new();
    h.enter("PAN");
    assert_eq!(h.prompt(), "Specify base point or displacement:");
    h.enter("0,0");
    assert_eq!(
        h.prompt(),
        "Specify second point <use first point as displacement>:"
    );
    let dirty = h.enter("100,50");
    assert!(dirty.contains(Dirty::VIEW));
    // The drawing appears to move right and up: the centre goes the other way.
    assert_eq!(h.session.view().center(), p(-100.0, -50.0));
    assert_eq!(h.session.view().scale(), 1.0);
    h.enter_all(&["P", "-100,-50", ""]);
    assert_eq!(h.session.view().center(), p(0.0, 0.0));
    assert!(!h.session.ui_state().can_undo);
}

#[test]
fn the_views_zoom_leaves_are_remembered_ten_deep() {
    let mut h = Harness::new();
    for _ in 0..15 {
        h.enter_all(&["Z", "S", "2"]);
    }
    for back in 1..=10 {
        h.enter_all(&["Z", "P"]);
        assert_eq!(h.session.view().scale(), 2f64.powi(15 - back));
    }
    h.enter_all(&["Z", "P"]);
    assert_eq!(h.log().last().unwrap(), "No previous view.");
    assert_eq!(h.session.view().scale(), 2f64.powi(5));
}

/// Zooming never touches the drawing's extents: the fit is of what is there.
#[test]
fn zoom_extents_after_an_edit_follows_the_drawing() {
    let mut h = far_line();
    h.enter_all(&["Z", "E"]);
    h.enter_all(&["C", "0,0", "10"]);
    h.enter_all(&["Z", "E"]);
    let bounds = Aabb::from_points(p(-10.0, -10.0), p(1400.0, 1300.0));
    assert_eq!(h.session.view().center(), bounds.center());
    assert!(h.session.view().visible_bounds().contains(&bounds));
}
