//! SNAP and grid snap through the session (#219): the command, F9, the
//! pointer landing on the grid, and its place in the pipeline — after
//! object snap, before ORTHO, never for a typed point.

use cad_core::Point2;
use cad_document::LineEntity;

use crate::testing::Harness;
use crate::{KeyAction, OverlayItem, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn aimed(h: &Harness) -> Point2 {
    h.session.aim().expect("a cursor").point
}

/// A session with no aid on.
fn bare() -> Harness {
    let mut h = Harness::new();
    h.toggle(Toggle::Osnap);
    h
}

#[test]
fn snap_sets_the_spacing_and_switches_grid_snap() {
    let mut h = bare();
    h.enter("SNAP");
    assert_eq!(
        h.prompt(),
        "Specify snap spacing or [ON/OFF/Aspect/Style] <10>:"
    );
    h.enter("-1");
    assert_eq!(
        h.log().last().unwrap(),
        "The spacing must be greater than zero."
    );
    h.enter("5");
    assert_eq!(h.log().last().unwrap(), "Snap spacing: 5");
    assert!(h.session.ui_state().toggles & Toggle::Snap.bit() != 0, "on");
    h.enter_all(&["SNAP", "OFF"]);
    assert_eq!(h.log().last().unwrap(), "<Snap off>");
    h.enter_all(&["SNAP", "A", "5", "2.5"]);
    assert_eq!(h.log().last().unwrap(), "Snap spacing: 5 across, 2.5 up");
    assert_eq!(h.session.settings().snap_spacing, [5.0, 2.5]);
    // F9 switches it, like every toggle.
    h.session.key(KeyAction::Toggle(Toggle::Snap));
    assert_eq!(h.log().last().unwrap(), "<SNAP off>");
}

#[test]
fn the_pointer_lands_on_the_grid_and_a_typed_point_does_not() {
    let mut h = bare();
    h.toggle(Toggle::Snap);
    h.move_to_world(p(21.3, 28.6));
    assert_eq!(aimed(&h), p(20.0, 30.0), "at the idle prompt too");
    h.enter("L");
    h.click_world(p(21.3, 28.6));
    h.click_world(p(58.0, -12.0));
    h.enter("21.3,28.6");
    h.enter("");
    let lines = h.lines();
    assert_eq!(lines[0], LineEntity::new(p(20.0, 30.0), p(60.0, -10.0)));
    assert_eq!(lines[1].end, p(21.3, 28.6), "typed points bypass every aid");
}

#[test]
fn object_snap_comes_first_and_ortho_works_from_the_snapped_point() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 0.0), p(23.0, 27.0)).into()]);
    h.toggle(Toggle::Snap);
    h.enter("L");
    // Near the line's end: the endpoint, not the grid.
    h.move_to_world(p(23.5, 26.5));
    assert_eq!(aimed(&h), p(23.0, 27.0));
    // Away from it, with ORTHO from a point on the grid.
    h.toggle(Toggle::Osnap);
    h.enter("100,100");
    h.toggle(Toggle::Ortho);
    h.move_to_world(p(141.0, 109.0));
    assert_eq!(aimed(&h), p(140.0, 100.0));
}

#[test]
fn the_grid_can_follow_the_snap_spacing() {
    let mut h = bare();
    h.toggle(Toggle::Grid);
    h.enter_all(&["SNAP", "2.5", "GRID"]);
    assert_eq!(h.prompt(), "Specify grid spacing or [Snap] <10>:");
    h.enter("S");
    assert_eq!(
        h.session.overlay()[0],
        OverlayItem::Grid {
            spacing: 2.5,
            isometric: false
        }
    );
    h.enter_all(&["GRID", "20"]);
    assert_eq!(
        h.session.overlay()[0],
        OverlayItem::Grid {
            spacing: 20.0,
            isometric: false
        }
    );
}
