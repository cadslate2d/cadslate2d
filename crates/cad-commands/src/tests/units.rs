//! UNITS and unit suffixes through the session (#113). The parsing is
//! tested in `cad-input/src/resolve.rs`; the setting, in
//! `cad-document/src/drawing.rs`.

use cad_core::{LengthUnit, Point2};
use cad_document::{CircleEntity, Entity, LineEntity};

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

/// A session whose drawing is in `unit`.
fn drawing_in(unit: &str) -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", unit]);
    assert_eq!(h.prompt(), "Command:");
    h
}

#[test]
fn units_sets_the_drawing_unit_and_is_undone_in_one_step() {
    let mut h = Harness::new();
    h.enter("UN");
    assert_eq!(
        h.prompt(),
        "Drawing unit? [Unitless/MM/CM/M/IN/FT] <Unitless>:"
    );
    h.enter("mm");
    assert_eq!(
        last(&h),
        "The drawing is in millimetres. Nothing was rescaled; a typed length such as 25mm is now converted."
    );
    assert_eq!(h.session.drawing().unit(), Some(LengthUnit::Millimetres));

    h.enter("UNITS");
    assert_eq!(h.prompt(), "Drawing unit? [Unitless/MM/CM/M/IN/FT] <MM>:");
    h.enter("M");
    assert_eq!(
        h.session.drawing().unit(),
        Some(LengthUnit::Metres),
        "M is metres"
    );

    h.undo();
    assert_eq!(h.session.drawing().unit(), Some(LengthUnit::Millimetres));
    h.undo();
    assert_eq!(h.session.drawing().unit(), None);
    h.redo();
    assert_eq!(h.session.drawing().unit(), Some(LengthUnit::Millimetres));

    h.enter_all(&["UNITS", "U"]);
    assert_eq!(
        last(&h),
        "The drawing is unitless: lengths are plain numbers."
    );
    assert_eq!(h.session.drawing().unit(), None);
}

#[test]
fn enter_keeps_the_unit_and_choosing_it_again_changes_nothing() {
    let mut h = drawing_in("FT");
    let revision = h.session.drawing().revision();
    h.enter_all(&["UNITS", ""]);
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["UNITS", "ft"]);
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "no transaction for no change"
    );
    assert_eq!(h.session.drawing().unit(), Some(LengthUnit::Feet));
}

#[test]
fn changing_the_unit_rescales_nothing() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "25,0", ""]);
    h.enter_all(&["UNITS", "MM"]);
    h.enter_all(&["UNITS", "IN"]);
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 25.0, 0.0)]);
}

/// The issue's criterion, through the whole program: 25mm in a metre
/// drawing is 0.025.
#[test]
fn typed_lengths_with_units_are_converted_into_the_drawings_unit() {
    let mut h = drawing_in("M");
    // Coordinates, relative coordinates, and a polar distance.
    h.enter_all(&["L", "0,0", "25mm,0", "@0,50cm", "@1in<180", ""]);
    let lines = h.lines();
    assert_eq!(lines[0].end, p(0.025, 0.0));
    assert_eq!(lines[1].end, p(0.025, 0.5));
    assert!((lines[2].end.x - (0.025 - 0.0254)).abs() < 1e-15);

    // A radius, and a bare number, which is still metres.
    h.enter_all(&["C", "5,5", "250mm", "C", "9,9", "2"]);
    let entities = h.entities();
    assert_eq!(entities[3], CircleEntity::new(p(5.0, 5.0), 0.25).into());
    assert_eq!(entities[4], CircleEntity::new(p(9.0, 9.0), 2.0).into());

    // A distance typed towards the pointer.
    h.enter_all(&["L", "0,10"]);
    h.move_to_world(p(50.0, 10.0));
    h.enter_all(&["300mm", ""]);
    assert_eq!(h.lines().last().unwrap().end, p(0.3, 10.0));

    // An offset distance and a fillet radius are lengths too.
    h.enter_all(&["O", "10cm"]);
    assert_eq!(h.session.settings().offset_distance, Some(0.1));
    h.escape();
    h.enter_all(&["F", "R", "1ft", ""]);
    assert!((h.session.settings().fillet_radius - 0.3048).abs() < 1e-15);
}

#[test]
fn a_unit_in_a_unitless_drawing_and_an_unknown_unit_are_explained() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "25mm,0"]);
    assert_eq!(
        last(&h),
        "This drawing has no unit, so \"25mm\" cannot be converted. Set one with UNITS."
    );
    assert_eq!(
        h.prompt(),
        "Specify next point or [Undo]:",
        "the command is still asking"
    );
    h.enter_all(&["25,0", ""]);
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 25.0, 0.0)]);

    let mut h = drawing_in("MM");
    h.enter_all(&["C", "0,0", "3furlongs"]);
    assert_eq!(
        last(&h),
        "\"furlongs\" is not a unit of length. Use mm, cm, m, in, or ft."
    );
    h.enter("3");
    assert_eq!(h.entities().len(), 1);
}

#[test]
fn a_scale_factor_and_an_angle_take_no_unit() {
    let mut h = drawing_in("MM");
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.select_box(p(-50.0, -50.0), p(50.0, 50.0));
    h.enter_all(&["SC", "0,0", "2mm"]);
    assert_eq!(last(&h), "A distance or a point is required.");
    h.enter("2");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 20.0, 0.0)]);

    h.select_box(p(-50.0, -50.0), p(50.0, 50.0));
    h.enter_all(&["RO", "0,0", "90mm"]);
    assert_eq!(last(&h), "An angle or a point is required.");
    h.enter("90");
    assert_eq!(h.entities(), vec![line(0.0, 0.0, 0.0, 20.0)]);
}
