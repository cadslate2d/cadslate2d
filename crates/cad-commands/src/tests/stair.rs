//! STAIR (#331) through the session: sized by Blondel's rule and said so,
//! drawn on STAIR in plan or section, scaled to the drawing's unit, turned
//! to the direction it climbs; one undo; a helix without headroom refused.

use cad_core::Point2;
use cad_document::Entity;

use crate::stair::STAIRS;
use crate::testing::Harness;

fn on_stair(h: &Harness) -> Vec<Entity> {
    let drawing = h.session.drawing();
    let Some(id) = drawing.layers().find(STAIRS) else {
        return Vec::new();
    };
    drawing
        .entities()
        .filter(|(e, _)| drawing.properties(*e).is_some_and(|p| p.layer == id))
        .map(|(_, e)| e.clone())
        .collect()
}

#[test]
fn a_straight_stair_in_plan_is_sized_said_and_drawn_as_one_undo() {
    let mut h = Harness::new();
    h.enter("STAIR");
    assert_eq!(h.prompt(), "Specify floor-to-floor height <2.8>:");
    h.enter_all(&["", ""]);
    assert_eq!(
        h.prompt(),
        "What kind of stair? [Straight/L/U/Helical] <Straight>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Which view? [Plan/Section/Across] <Plan>:");
    h.enter_all(&["", "", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(
        h.log().last().unwrap(),
        "16 risers of 17.5 cm and 15 treads of 28.0 cm: two risers and a tread, 63.0 cm."
    );
    let drawn = on_stair(&h);
    // The outline, 14 lines between treads, the walking line and its arrow,
    // the cut line, the label.
    assert_eq!(drawn.len(), 19);
    let lines = drawn
        .iter()
        .filter(|e| matches!(e, Entity::Line(_)))
        .count();
    assert_eq!(lines, 15, "14 treads' lines and the walking line");
    h.undo();
    assert!(on_stair(&h).is_empty());

    // Climbing east, its label reads along it: the stair's frame turned a
    // quarter clockwise, in radians.
    h.enter_all(&["STAIR", "", "", "", "", "", "0"]);
    let label = on_stair(&h)
        .into_iter()
        .find_map(|e| match e {
            Entity::Text(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!((label.rotation + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn a_u_stair_in_section_in_centimetres_turned_climbs_its_height() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "CM", "STAIR"]);
    assert_eq!(h.prompt(), "Specify floor-to-floor height <280>:");
    h.enter_all(&["300", "100", "U", "S", "100,50", "0"]);
    assert_eq!(h.prompt(), "Command:");
    let message = h.log().last().unwrap().clone();
    assert!(message.contains("risers of"), "{message}");
    // Cut along its flights (#358): the first flight and the landing one
    // closed outline from the foot at 100,50, along +x; the second flight
    // beyond reaching the floor above, 300 cm higher, inside the same run.
    let drawn = on_stair(&h);
    let cut = drawn
        .iter()
        .find_map(|e| match e {
            Entity::Polyline(p) if p.closed && p.vertices.len() > 20 => Some(p),
            _ => None,
        })
        .expect("the first flight and landing, cut");
    let run = cut
        .vertices
        .iter()
        .map(|v| v.x)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(cut.vertices.iter().all(|v| v.x >= 100.0 - 1e-6));
    let beyond = drawn
        .iter()
        .find_map(|e| match e {
            Entity::Polyline(p)
                if !p.closed
                    && p.vertices.len() > 4
                    && p.vertices.iter().any(|v| (v.y - 350.0).abs() < 1e-6) =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("the second flight beyond");
    assert!(
        beyond
            .vertices
            .iter()
            .all(|v| v.x >= 100.0 - 1e-6 && v.x <= run + 1e-6)
    );
    let _ = Point2::ORIGIN;
}

/// A U cut across its flights (#358): each flight a block at its height
/// where the cut meets it, side by side, the first to the right.
#[test]
fn a_u_stair_cut_across_is_two_flights_side_by_side() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "M", "STAIR", "3.2", "1.2", "U", "A"]);
    assert!(
        h.prompt()
            .starts_with("Specify how far along the climb to cut <"),
        "{}",
        h.prompt()
    );
    // Well up the first flight, where the second flight is cut too.
    h.enter_all(&["1.8", "0,0", ""]);
    assert_eq!(h.prompt(), "Command:");
    let blocks: Vec<Vec<Point2>> = on_stair(&h)
        .into_iter()
        .filter_map(|e| match e {
            Entity::Polyline(p) if p.closed && p.vertices.len() == 4 => Some(p.vertices),
            _ => None,
        })
        .collect();
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    let top = |b: &[Point2]| b.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max);
    let right = |b: &[Point2]| b.iter().map(|v| v.x).fold(f64::NEG_INFINITY, f64::max);
    let (first, second) = if right(&blocks[0]) > 0.0 {
        (&blocks[0], &blocks[1])
    } else {
        (&blocks[1], &blocks[0])
    };
    assert!((right(first) - 0.6).abs() < 1e-9 && (right(second) + 0.6).abs() < 1e-9);
    // The second flight, coming back over the first, is the higher there.
    assert!(top(second) > top(first) && top(second) < 3.2);
}

#[test]
fn a_helix_without_headroom_is_refused_and_asked_again() {
    let mut h = Harness::new();
    h.enter_all(&["STAIR", "", "0.6", "H", "0.1"]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("A full turn climbs only"),
        "{:?}",
        h.log()
    );
    assert_eq!(h.prompt(), "Specify stair width <0.6>:");
    h.enter_all(&["1", "H", "0.15", "", "", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert!(
        on_stair(&h).iter().any(|e| matches!(e, Entity::Circle(_))),
        "the newel"
    );
}
