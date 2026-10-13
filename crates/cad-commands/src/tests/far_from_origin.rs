//! Drafting far from the origin (#151; ADR-0012): the whole program — typed
//! coordinates, snaps, and every modify command — on a plot 10⁷ and 10¹⁰
//! units out, held to the tolerance the numbers there allow.

use std::f64::consts::FRAC_PI_2;

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};
use cad_document::{ArcEntity, CircleEntity, Drawing, Entity, LineEntity};

use crate::snap::SnapMode;
use crate::testing::Harness;

const OFFSETS: [f64; 2] = [1e7, 1e10];

/// A harness on an empty drawing whose view sits over a plot at `offset`.
fn at(offset: f64) -> (Harness, Point2) {
    let o = Point2::new(offset, offset);
    let mut h = Harness::new();
    // A corner post of the plot, so the view fits around it; the origin
    // of the view is then there, as it is after OPEN.
    let post = Entity::from(LineEntity::new(
        o + Vec2::new(-20.0, -20.0),
        o + Vec2::new(220.0, 120.0),
    ));
    h.session.open_drawing(
        Drawing::from_entities([post]),
        "far.dxf",
        Vec::new(),
        Vec::new(),
    );
    (h, o)
}

fn tolerance(points: &[Point2]) -> LinearTolerance {
    LinearTolerance::DEFAULT.around(points)
}

fn same(a: Point2, b: Point2) -> bool {
    tolerance(&[a, b]).coincident(a, b)
}

fn lines_of(h: &Harness) -> Vec<LineEntity> {
    h.entities()
        .iter()
        .filter_map(|entity| entity.as_line().copied())
        .collect()
}

fn coords(o: Point2, x: f64, y: f64) -> String {
    format!("{},{}", o.x + x, o.y + y)
}

#[test]
fn lines_are_drawn_where_they_are_typed_and_relative_to_the_last_point() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        h.enter_all(&["L", &coords(o, 0.0, 0.0), "@100,0", "@0,50", ""]);
        let lines = lines_of(&h);
        assert_eq!(lines.len(), 3, "at {offset}");
        assert!(same(lines[1].start, o), "at {offset}: {:?}", lines[1].start);
        assert!(same(lines[1].end, o + Vec2::new(100.0, 0.0)), "at {offset}");
        assert!(
            same(lines[2].end, o + Vec2::new(100.0, 50.0)),
            "at {offset}"
        );
    }
}

#[test]
fn snaps_find_the_exact_endpoint_midpoint_and_intersection() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        h.enter_all(&["L", &coords(o, 0.0, 0.0), &coords(o, 100.0, 0.0), ""]);
        h.enter_all(&["L", &coords(o, 40.0, -30.0), &coords(o, 40.0, 30.0), ""]);
        h.enter("L");
        for (mode, point, near) in [
            (
                SnapMode::Endpoint,
                o + Vec2::new(100.0, 0.0),
                Vec2::new(-1.0, 0.5),
            ),
            (
                SnapMode::Midpoint,
                o + Vec2::new(50.0, 0.0),
                Vec2::new(0.4, -0.6),
            ),
            (
                SnapMode::Intersection,
                o + Vec2::new(40.0, 0.0),
                Vec2::new(0.3, 0.4),
            ),
        ] {
            h.move_to_world(point + near);
            let snap = h
                .session
                .snap()
                .unwrap_or_else(|| panic!("at {offset}: no {mode:?} snap"));
            assert_eq!(snap.mode, mode, "at {offset}");
            assert_eq!(snap.point, point, "at {offset}: the exact point");
        }
        // The click commits the snapped point exactly.
        h.move_to_world(o + Vec2::new(99.0, 0.5));
        h.click_world(o + Vec2::new(99.0, 0.5));
        h.enter_all(&["@0,10", ""]);
        let lines = lines_of(&h);
        let drawn = lines.last().unwrap();
        assert_eq!(drawn.start, o + Vec2::new(100.0, 0.0), "at {offset}");
    }
}

#[test]
fn trim_and_extend_meet_the_edge() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        let edge = o + Vec2::new(40.0, 0.0);
        h.enter_all(&["L", &coords(o, 40.0, -60.0), &coords(o, 40.0, 60.0), ""]);
        h.enter_all(&["L", &coords(o, 0.0, 0.0), &coords(o, 100.0, 0.0), ""]);
        h.enter_all(&["L", &coords(o, 0.0, 20.0), &coords(o, 30.0, 20.0), ""]);
        // TRIM the crossing line's right part at the edge.
        h.enter("TR");
        h.click_world(o + Vec2::new(40.0, 30.0));
        h.enter("");
        h.click_world(o + Vec2::new(80.0, 0.0));
        h.enter("");
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        // EXTEND the short line to the edge.
        h.enter("EX");
        h.click_world(o + Vec2::new(40.0, 30.0));
        h.enter("");
        h.click_world(o + Vec2::new(25.0, 20.0));
        h.enter("");
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        let lines = lines_of(&h);
        let trimmed = lines
            .iter()
            .find(|line| same(line.start, o))
            .unwrap_or_else(|| panic!("at {offset}: the trimmed line"));
        assert!(
            same(trimmed.end, edge),
            "at {offset}: trimmed to {:?}",
            trimmed.end
        );
        let extended = lines
            .iter()
            .find(|line| same(line.start, o + Vec2::new(0.0, 20.0)))
            .unwrap_or_else(|| panic!("at {offset}: the extended line"));
        assert!(
            same(extended.end, o + Vec2::new(40.0, 20.0)),
            "at {offset}: extended to {:?}",
            extended.end
        );
    }
}

#[test]
fn fillet_rounds_the_corner_with_the_radius_asked() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        h.enter_all(&["L", &coords(o, 10.0, 0.0), &coords(o, 100.0, 0.0), ""]);
        h.enter_all(&["L", &coords(o, 0.0, 10.0), &coords(o, 0.0, 80.0), ""]);
        h.enter_all(&["F", "R", "10"]);
        h.click_world(o + Vec2::new(50.0, 0.0));
        h.click_world(o + Vec2::new(0.0, 50.0));
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        let arc = h
            .entities()
            .iter()
            .find_map(|entity| match entity {
                Entity::Arc(arc) => Some(*arc),
                _ => None,
            })
            .unwrap_or_else(|| panic!("at {offset}: no arc"));
        let tol = tolerance(&[o]);
        assert!(
            same(arc.center, o + Vec2::new(10.0, 10.0)),
            "at {offset}: {:?}",
            arc.center
        );
        assert!(
            tol.eq(arc.radius, 10.0),
            "at {offset}: radius {}",
            arc.radius
        );
        let lines = lines_of(&h);
        assert!(
            lines
                .iter()
                .any(|line| same(line.start, o + Vec2::new(10.0, 0.0))
                    || same(line.end, o + Vec2::new(10.0, 0.0))),
            "at {offset}: the horizontal line ends at the arc"
        );
    }
}

#[test]
fn offset_break_join_and_stretch_hold_their_geometry() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        // OFFSET a line by 10, to the upper side.
        h.enter_all(&["L", &coords(o, 0.0, 0.0), &coords(o, 100.0, 0.0), ""]);
        h.enter_all(&["O", "10"]);
        h.click_world(o + Vec2::new(50.0, 0.0));
        h.click_world(o + Vec2::new(50.0, 30.0));
        h.enter("");
        let lines = lines_of(&h);
        assert!(
            lines
                .iter()
                .any(|line| same(line.start, o + Vec2::new(0.0, 10.0))
                    && same(line.end, o + Vec2::new(100.0, 10.0))),
            "at {offset}: offset line among {lines:?}"
        );
        // BREAK the original between 30 and 70.
        h.enter("BR");
        h.click_world(o + Vec2::new(30.0, 0.0));
        h.click_world(o + Vec2::new(70.0, 0.0));
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        let lines = lines_of(&h);
        assert!(
            lines
                .iter()
                .any(|line| same(line.end, o + Vec2::new(30.0, 0.0))),
            "at {offset}: break start"
        );
        assert!(
            lines
                .iter()
                .any(|line| same(line.start, o + Vec2::new(70.0, 0.0))),
            "at {offset}: break end"
        );
        // JOIN the two pieces back.
        h.enter("J");
        h.click_world(o + Vec2::new(10.0, 0.0));
        h.click_world(o + Vec2::new(90.0, 0.0));
        h.enter("");
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        let lines = lines_of(&h);
        assert!(
            lines
                .iter()
                .any(|line| same(line.start, o) && same(line.end, o + Vec2::new(100.0, 0.0))),
            "at {offset}: joined among {lines:?}"
        );
        // STRETCH the right ends of both lines by 50.
        h.enter_all(&[
            "S",
            &coords(o, 80.0, -20.0),
            &coords(o, 130.0, 30.0),
            &coords(o, 0.0, 0.0),
            &coords(o, 50.0, 0.0),
        ]);
        assert_eq!(h.prompt(), "Command:", "at {offset}: {:?}", h.log().last());
        let lines = lines_of(&h);
        assert!(
            lines
                .iter()
                .any(|line| same(line.end, o + Vec2::new(150.0, 0.0))),
            "at {offset}: stretched among {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| same(line.end, o + Vec2::new(150.0, 10.0))),
            "at {offset}: the offset line stretched too"
        );
    }
}

#[test]
fn a_circle_and_an_arc_are_picked_and_snapped_to_far_out() {
    for offset in OFFSETS {
        let (mut h, o) = at(offset);
        h.enter_all(&["C", &coords(o, 50.0, 50.0), "20"]);
        h.enter_all(&[
            "A",
            &coords(o, 100.0, 0.0),
            &coords(o, 150.0, 50.0),
            &coords(o, 100.0, 100.0),
        ]);
        let entities = h.entities();
        let circle: CircleEntity = entities
            .iter()
            .find_map(|e| match e {
                Entity::Circle(circle) => Some(*circle),
                _ => None,
            })
            .unwrap();
        assert!(
            same(circle.center, o + Vec2::new(50.0, 50.0)),
            "at {offset}"
        );
        let arc: ArcEntity = entities
            .iter()
            .find_map(|e| match e {
                Entity::Arc(arc) => Some(*arc),
                _ => None,
            })
            .unwrap();
        assert!(
            same(arc.center, o + Vec2::new(100.0, 50.0)),
            "at {offset}: {:?}",
            arc.center
        );
        assert!(tolerance(&[o]).eq(arc.radius, 50.0), "at {offset}");
        let sweep = arc.arc().sweep;
        assert!(
            (sweep - FRAC_PI_2 * 2.0).abs() < 1e-6,
            "at {offset}: sweep {sweep}"
        );
        // Centre and quadrant snaps.
        h.enter("L");
        h.move_to_world(o + Vec2::new(50.5, 49.6));
        let snap = h.session.snap().unwrap();
        assert_eq!(
            (snap.mode, snap.point),
            (SnapMode::Center, o + Vec2::new(50.0, 50.0)),
            "at {offset}"
        );
        h.move_to_world(o + Vec2::new(70.4, 50.3));
        let snap = h.session.snap().unwrap();
        assert_eq!(snap.mode, SnapMode::Quadrant, "at {offset}");
        assert!(
            same(snap.point, o + Vec2::new(70.0, 50.0)),
            "at {offset}: {:?}",
            snap.point
        );
        h.escape();
    }
}
