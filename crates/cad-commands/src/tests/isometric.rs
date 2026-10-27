//! Isometric drafting through the session (#336): SNAP's isometric style,
//! the pointer on the lattice, ORTHO along the current plane, ISOPLANE and
//! F5, the isometric grid, and ELLIPSE's isocircles, one undo each.

use cad_core::iso::{self, Plane};
use cad_core::{Point2, Vec2};
use cad_document::Entity;

use crate::testing::Harness;
use crate::{KeyAction, OverlayItem, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn aimed(h: &Harness) -> Point2 {
    h.session.aim().expect("a cursor").point
}

/// Isometric snap at 10, every other aid off.
fn isometric() -> Harness {
    let mut h = Harness::new();
    h.toggle(Toggle::Osnap);
    h.enter_all(&["SNAP", "S"]);
    assert_eq!(
        h.prompt(),
        "Enter snap grid style or [Standard/Isometric] <Standard>:"
    );
    h.enter_all(&["I", "10"]);
    h
}

#[test]
fn the_isometric_style_puts_the_pointer_on_the_lattice() {
    let mut h = isometric();
    assert_eq!(
        h.log().last().unwrap(),
        "Snap style: Isometric, 10 up. Isoplane: Top"
    );
    assert!(h.session.ui_state().toggles & Toggle::Snap.bit() != 0);
    assert_eq!(h.session.ui_state().isoplane, Some("Top"));
    // Near one step along x, the pointer is on it: (8.66, 5).
    h.move_to_world(p(9.5, 4.0));
    assert!(aimed(&h).distance_to(iso::map(10.0, 0.0, 0.0)) < 1e-9);
    // Straight up a column, every 10.
    h.move_to_world(p(0.4, 31.0));
    assert!(aimed(&h).distance_to(p(0.0, 30.0)) < 1e-9);
    // Back to the standard style: the square grid again.
    h.enter_all(&["SNAP", "S", "S"]);
    assert_eq!(h.log().last().unwrap(), "Snap style: Standard");
    assert_eq!(h.session.ui_state().isoplane, None);
    h.move_to_world(p(9.5, 4.0));
    assert_eq!(aimed(&h), p(10.0, 0.0));
}

#[test]
fn ortho_follows_the_current_plane_and_f5_and_isoplane_switch_it() {
    let mut h = isometric();
    h.toggle(Toggle::Snap);
    h.toggle(Toggle::Ortho);
    h.enter("LINE");
    h.enter("0,0");
    // Up and to the right: along x on the top plane.
    h.move_to_world(p(20.0, 14.0));
    let at = aimed(&h);
    assert!(
        (at.y / at.x - 30_f64.to_radians().tan()).abs() < 1e-9,
        "{at:?}"
    );
    // F5: the right plane, x and z; straight up is z.
    h.session.key(KeyAction::Isoplane);
    assert_eq!(h.log().last().unwrap(), "<Isoplane Right>");
    h.move_to_world(p(3.0, 20.0));
    assert!(aimed(&h).x.abs() < 1e-9);
    // F5 again: left, y and z; up and to the left is y, along 150°.
    h.session.key(KeyAction::Isoplane);
    h.move_to_world(p(-20.0, 13.0));
    let at = aimed(&h);
    assert!(
        (at.y / at.x + 30_f64.to_radians().tan()).abs() < 1e-9,
        "{at:?}"
    );
    h.enter("");
    // ISOPLANE by name, and Enter for the next.
    h.enter_all(&["ISOPLANE"]);
    assert_eq!(
        h.prompt(),
        "Enter isometric plane setting or [Left/Top/Right] <Top>:"
    );
    h.enter("R");
    assert_eq!(h.log().last().unwrap(), "Current isoplane: Right");
    h.enter_all(&["ISOPLANE", ""]);
    assert_eq!(h.session.settings().isoplane, Plane::Left);
    assert_eq!(h.session.ui_state().isoplane, Some("Left"));
}

#[test]
fn the_grid_is_drawn_isometric() {
    let mut h = isometric();
    h.toggle(Toggle::Grid);
    assert!(h.session.overlay().contains(&OverlayItem::Grid {
        spacing: 10.0,
        isometric: true
    }));
}

#[test]
fn an_isocircle_is_the_circle_of_the_current_plane_as_one_undo() {
    let mut h = isometric();
    h.toggle(Toggle::Snap);
    // Without the isometric style, ELLIPSE offers no isocircle.
    let mut plain = Harness::new();
    plain.enter("ELLIPSE");
    assert_eq!(
        plain.prompt(),
        "Specify axis endpoint of ellipse or [Arc/Center]:"
    );
    h.enter("ELLIPSE");
    assert_eq!(
        h.prompt(),
        "Specify axis endpoint of ellipse or [Arc/Center/Isocircle]:"
    );
    h.enter_all(&["I", "50,40"]);
    assert_eq!(h.prompt(), "Specify radius of isocircle or [Diameter]:");
    h.enter("10");
    let Entity::Ellipse(top) = h.entities()[0].clone() else {
        panic!("{:?}", h.entities());
    };
    assert_eq!(top.center, p(50.0, 40.0));
    let (major, ratio) = Plane::Top.circle(10.0);
    assert!((top.major - major).length() < 1e-9 && (top.ratio - ratio).abs() < 1e-12);
    // The right plane's, by its diameter: the long axis at 60°.
    h.enter_all(&["ISOPLANE", "R", "ELLIPSE", "I", "0,0", "D", "20"]);
    let Entity::Ellipse(right) = h.entities()[1].clone() else {
        panic!()
    };
    let angle = right
        .major
        .y
        .atan2(right.major.x)
        .to_degrees()
        .rem_euclid(180.0);
    assert!((angle - 60.0).abs() < 1e-9, "{angle}");
    assert!((right.major.length() - 10.0 * 1.5_f64.sqrt()).abs() < 1e-9);
    // It passes through the middles of the plane's square's sides.
    let side = Vec2::new(0.0, 10.0);
    let minor = Vec2::new(-right.major.y, right.major.x) * right.ratio;
    let u = side.dot(right.major) / right.major.length_squared();
    let v = side.dot(minor) / minor.length_squared();
    assert!((u * u + v * v - 1.0).abs() < 1e-9);
    h.undo();
    assert_eq!(h.entities().len(), 1);
}

#[test]
fn text_on_a_plane_is_a_style_leaning_30_degrees_turned_30() {
    // Text on the right plane: a style obliqued 30°, the text turned 30°.
    // Its baseline runs along x and its uprights along z.
    let mut h = isometric();
    h.enter_all(&["STYLE", "N", "ISO-RIGHT", "O", "30", ""]);
    h.enter_all(&["TEXT", "0,0", "5", "30", "AB"]);
    let Some(Entity::Text(text)) = h.entities().into_iter().next() else {
        panic!("{:?}", h.log());
    };
    let along = Vec2::from_polar(1.0, text.rotation);
    assert!((along - iso::X).length() < 1e-9);
    // Leaning 30° from upright, the uprights run along z.
    let drawing = h.session.drawing();
    let style = drawing.text_styles().get(text.style).expect("its style");
    assert_eq!(style.name, "ISO-RIGHT");
    assert!(
        (style.oblique - 30_f64.to_radians()).abs() < 1e-9,
        "{}",
        style.oblique
    );
}
