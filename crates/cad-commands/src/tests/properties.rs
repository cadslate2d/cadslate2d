//! What each modify command does with an object's properties (#131,
//! ADR-0013; the rule is published in `docs/modify-matrix.md`):
//!
//! - an object that is changed keeps its own;
//! - an object made from another takes that object's;
//! - a fillet arc or a chamfer bevel takes those of the object picked first;
//! - a joined object takes those of its oldest member;
//! - a newly drawn object gets the properties new objects are given.

use cad_core::Point2;
use cad_document::{
    ArcEntity, CircleEntity, Color, Drawing, Entity, Layer, LayerId, LineEntity, Lineweight,
    PolylineEntity, Properties,
};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

const WALLS: LayerId = LayerId::from_raw(1);
const DOORS: LayerId = LayerId::from_raw(2);

/// Red, on the walls layer.
fn red() -> Properties {
    Properties {
        color: Color::Index(1),
        ..Properties::on(WALLS)
    }
}

/// Blue and with its own lineweight, on the doors layer.
fn blue() -> Properties {
    Properties {
        color: Color::Index(5),
        lineweight: Lineweight::Default,
        ..Properties::on(DOORS)
    }
}

/// A drawing with two layers besides `0`, holding `entities` as styled.
fn styled(entities: impl IntoIterator<Item = (Entity, Properties)>) -> Harness {
    let drawing = Drawing::from_parts([Layer::named("walls"), Layer::named("doors")], entities)
        .expect("the layers exist");
    Harness::with_drawing(drawing)
}

/// One red horizontal line and one blue vertical one, crossing at (40, 0).
fn cross() -> Harness {
    styled([
        (line(0.0, 0.0, 100.0, 0.0), red()),
        (line(40.0, -50.0, 40.0, 50.0), blue()),
    ])
}

fn select_all(h: &mut Harness) {
    h.select_box(p(-300.0, -300.0), p(300.0, 300.0));
}

#[test]
fn a_newly_drawn_object_gets_default_properties() {
    let mut h = cross();
    h.enter_all(&[
        "L", "0,80", "50,80", "", "C", "0,0", "5", "PL", "0,90", "9,90", "9,99", "",
    ]);
    h.enter_all(&[
        "A", "0,-80", "10,-70", "20,-80", "REC", "200,200", "210,210",
    ]);
    let properties = h.properties();
    assert_eq!(properties.len(), 7);
    assert_eq!(&properties[..2], &[red(), blue()]);
    assert!(properties[2..].iter().all(|p| *p == Properties::default()));
}

#[test]
fn move_rotate_scale_and_mirror_keep_each_objects_own() {
    for command in [
        &["M", "0,0", "10,10"][..],
        &["RO", "0,0", "90"],
        &["SC", "0,0", "2"],
        &["MI", "0,0", "0,10", "Y"],
    ] {
        let mut h = cross();
        let before = h.entities();
        select_all(&mut h);
        h.enter_all(command);
        assert_eq!(h.prompt(), "Command:", "{command:?}");
        assert_ne!(h.entities(), before, "{command:?} changed the geometry");
        assert_eq!(h.properties(), vec![red(), blue()], "{command:?}");
    }
}

#[test]
fn copies_and_mirror_images_take_their_sources() {
    let mut h = cross();
    select_all(&mut h);
    h.enter_all(&["CO", "0,0", "0,100", ""]);
    assert_eq!(h.properties(), vec![red(), blue(), red(), blue()]);

    let mut h = cross();
    select_all(&mut h);
    h.enter_all(&["MI", "200,0", "200,10", "N"]);
    assert_eq!(h.properties(), vec![red(), blue(), red(), blue()]);
}

#[test]
fn an_offset_takes_its_sources() {
    let mut h = styled([
        (line(0.0, 0.0, 100.0, 0.0), red()),
        (CircleEntity::new(p(0.0, 100.0), 20.0).into(), blue()),
        (
            PolylineEntity::new(
                vec![p(0.0, -100.0), p(50.0, -100.0), p(50.0, -150.0)],
                false,
            )
            .into(),
            blue(),
        ),
    ]);
    h.enter_all(&["O", "5"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(50.0, 30.0));
    h.click_world(p(20.0, 100.0));
    h.click_world(p(60.0, 100.0));
    h.click_world(p(25.0, -100.0));
    h.click_world(p(25.0, -80.0));
    h.enter("");
    assert_eq!(
        h.properties(),
        vec![red(), blue(), blue(), red(), blue(), blue()]
    );
}

#[test]
fn every_piece_of_a_trimmed_or_broken_object_keeps_its_properties() {
    // TRIM the middle out of the red line, between two blue cutters.
    let mut h = styled([
        (line(0.0, 0.0, 100.0, 0.0), red()),
        (line(30.0, -50.0, 30.0, 50.0), blue()),
        (line(70.0, -50.0, 70.0, 50.0), blue()),
    ]);
    h.enter_all(&["TR", ""]);
    h.click_world(p(50.0, 0.0));
    h.enter("");
    assert_eq!(h.entities().len(), 4);
    assert_eq!(h.properties(), vec![red(), blue(), blue(), red()]);

    // BREAK the red line in two.
    let mut h = cross();
    h.enter("BR");
    h.click_world(p(20.0, 0.0));
    h.enter("@");
    assert_eq!(h.entities().len(), 3);
    assert_eq!(h.properties(), vec![red(), blue(), red()]);
}

#[test]
fn extend_stretch_and_a_grip_edit_keep_the_objects_own() {
    let mut h = styled([
        (line(0.0, 0.0, 30.0, 0.0), red()),
        (line(40.0, -50.0, 40.0, 50.0), blue()),
    ]);
    h.enter_all(&["EX", ""]);
    h.click_world(p(25.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[0], line(0.0, 0.0, 40.0, 0.0));
    assert_eq!(h.properties(), vec![red(), blue()]);

    h.enter_all(&["S", "30,40", "50,60", "0,0", "0,20"]);
    assert_eq!(h.entities()[1], line(40.0, -50.0, 40.0, 70.0));
    assert_eq!(h.properties(), vec![red(), blue()]);

    h.click_world(p(10.0, 0.0)); // select the red line
    h.click_world(p(0.0, 0.0)); // take its start grip
    h.enter("0,-20");
    assert_eq!(h.entities()[0], line(0.0, -20.0, 40.0, 0.0));
    assert_eq!(h.properties(), vec![red(), blue()]);
}

#[test]
fn a_fillet_arc_and_a_chamfer_bevel_take_the_first_objects() {
    let corner = || {
        styled([
            (line(10.0, 0.0, 100.0, 0.0), red()),
            (line(0.0, 10.0, 0.0, 80.0), blue()),
        ])
    };

    let mut h = corner();
    h.enter_all(&["F", "R", "10"]);
    h.click_world(p(50.0, 0.0)); // the red line first
    h.click_world(p(0.0, 50.0));
    assert!(matches!(h.entities()[2], Entity::Arc(_)));
    assert_eq!(h.properties(), vec![red(), blue(), red()]);

    let mut h = corner();
    h.enter_all(&["F", "R", "10"]);
    h.click_world(p(0.0, 50.0)); // the blue line first
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.properties(), vec![red(), blue(), blue()]);

    let mut h = corner();
    h.enter_all(&["CHA", "D", "10", ""]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 50.0));
    assert!(matches!(h.entities()[2], Entity::Line(_)));
    assert_eq!(h.properties(), vec![red(), blue(), red()]);

    // Rounding a polyline's own corner changes the polyline and nothing else.
    let mut h = styled([(
        PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], false).into(),
        blue(),
    )]);
    h.enter_all(&["F", "R", "10"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(100.0, 50.0));
    assert_eq!(h.entities().len(), 1);
    assert_eq!(h.properties(), vec![blue()]);
}

#[test]
fn a_joined_object_takes_its_oldest_members() {
    // Collinear lines → a line; an arc and lines → a polyline; arcs → an arc.
    let half = |from: f64, to: f64| -> Entity {
        ArcEntity::new(p(0.0, 200.0), 30.0, from.to_radians(), to.to_radians()).into()
    };
    let cases = [
        vec![
            (line(0.0, 0.0, 30.0, 0.0), blue()),
            (line(70.0, 0.0, 100.0, 0.0), red()),
        ],
        vec![
            (line(0.0, 0.0, 50.0, 0.0), red()),
            (line(50.0, 0.0, 50.0, 50.0), blue()),
            (line(50.0, 50.0, 90.0, 90.0), Properties::default()),
        ],
        vec![(half(0.0, 90.0), blue()), (half(90.0, 200.0), red())],
    ];
    for entities in cases {
        let oldest = entities[0].1;
        let mut h = styled(entities);
        select_all(&mut h);
        h.enter("J");
        assert_eq!(h.entities().len(), 1, "{:?}", h.log().last());
        assert_eq!(h.properties(), vec![oldest]);
    }
}

#[test]
fn erase_and_undo_bring_an_object_back_as_it_was() {
    let mut h = cross();
    select_all(&mut h);
    h.enter("E");
    assert!(h.entities().is_empty());
    h.undo();
    assert_eq!(h.properties(), vec![red(), blue()]);

    // And every command above is undone to the properties there were.
    select_all(&mut h);
    h.enter_all(&["CO", "0,0", "0,100", ""]);
    h.undo();
    assert_eq!(h.properties(), vec![red(), blue()]);
}
