//! WALLS (#358) through the session: walls with their thickness, one wall
//! where walls meet, the openings cut and filled; by hand, as JSON, and by
//! an intent inside a sheet's view — never a centreline left between faces.

use cad_core::Point2;
use cad_document::Entity;
use cad_geometry::Segment;
use cad_intent::{VERSION, parse};

use crate::testing::Harness;

fn lines(h: &Harness) -> Vec<Segment> {
    h.entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Line(l) => Some(Segment::new(l.start, l.end)),
            _ => None,
        })
        .collect()
}

/// A line lying along the segment from `a` to `b`, any part of it.
fn along(lines: &[Segment], a: Point2, b: Point2) -> bool {
    let d = b - a;
    let n = d.length();
    lines.iter().any(|s| {
        let (u, v) = (s.start - a, s.end - a);
        (d.x * u.y - d.y * u.x).abs() / n < 1e-6 && (d.x * v.y - d.y * v.x).abs() / n < 1e-6
    })
}

#[test]
fn walls_by_hand_are_a_closed_chain_of_faces_as_one_undo() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm", "WALLS"]);
    assert_eq!(h.prompt(), "Specify wall thickness or [Json] <200>:");
    h.enter_all(&["", "0,0", "7000,0", "7000,15000", "0,15000", "C"]);
    assert_eq!(h.prompt(), "Command:");
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("4 walls drawn as WALLS-1: 8 faces"),
        "{:?}",
        h.log()
    );
    let drawn = lines(&h);
    // Outside and inside, a face each side, nothing along the centreline.
    assert!(along(
        &drawn,
        Point2::new(-100.0, -100.0),
        Point2::new(7100.0, -100.0)
    ));
    assert!(along(
        &drawn,
        Point2::new(100.0, 100.0),
        Point2::new(6900.0, 100.0)
    ));
    assert!(!along(
        &drawn,
        Point2::new(0.0, 0.0),
        Point2::new(7000.0, 0.0)
    ));
    h.undo();
    assert_eq!(h.session.drawing().len(), 0);
}

/// The house's ground floor (#358), its walls in metres as an intent
/// gives them, in a sheet's view: 200 mm outside, a 150 mm partition with
/// a door, a window in the front. Every face where the centrelines and
/// thicknesses put it; no line along a centreline.
#[test]
fn a_floor_by_intent_in_a_view_has_every_wall_its_thickness() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    let document = parse(&format!(
        r#"{{"version": {VERSION}, "units": "m", "operations": [
        {{"op": "title_block", "paper": "A2", "portrait": true, "scale": 50, "title": "GROUND FLOOR", "at": [0, 0]}},
        {{"op": "view", "id": "GF", "placed": "principal", "operations": [
            {{"op": "layer", "name": "A-WALL", "current": true}},
            {{"op": "walls", "walls": [
                {{"from": [1.5, 5], "to": [8.5, 5], "thickness": 0.2}},
                {{"from": [8.5, 5], "to": [8.5, 20], "thickness": 0.2}},
                {{"from": [8.5, 20], "to": [1.5, 20], "thickness": 0.2}},
                {{"from": [1.5, 20], "to": [1.5, 5], "thickness": 0.2}},
                {{"from": [5.2, 10], "to": [5.2, 14], "thickness": 0.15}},
                {{"from": [5.2, 10], "to": [8.5, 10], "thickness": 0.15}}],
              "openings": [
                {{"wall": 0, "at": 5.5, "width": 0.9, "kind": "door", "swing": "left"}},
                {{"wall": 0, "at": 2.0, "width": 1.5, "kind": "window"}},
                {{"wall": 5, "at": 1.0, "width": 0.8, "kind": "door", "swing": "right", "hinge": "end"}}],
              "door_layer": "A-DOOR", "window_layer": "A-WINDOW"}}]}}]}}"#
    ))
    .unwrap();
    h.session
        .run_intent(&document)
        .unwrap_or_else(|e| panic!("{e:?}: {:?}", h.log()));
    // Where the view went: its walls' group, measured from its own origin.
    let drawing = h.session.drawing();
    let group = drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name.starts_with("WALLS-"))
        .map(|(_, g)| g.clone())
        .expect("the walls' group");
    let sheet = crate::compose::sheet_of(drawing).unwrap();
    let base = crate::compose::views_on(drawing, &sheet.group)
        .into_iter()
        .find(|v| v.id == "GF")
        .unwrap()
        .origin;
    let wall_layer = drawing.layers().find("A-WALL").unwrap();
    let faces: Vec<Segment> = group
        .members
        .iter()
        .filter(|id| {
            drawing
                .properties(**id)
                .is_some_and(|p| p.layer == wall_layer)
        })
        .filter_map(|id| match drawing.entity(*id) {
            Some(Entity::Line(l)) => Some(Segment::new(
                l.start - (base - Point2::ORIGIN),
                l.end - (base - Point2::ORIGIN),
            )),
            _ => None,
        })
        .collect();
    let p = |x: f64, y: f64| Point2::new(x * 1000.0, y * 1000.0);
    // The outside walls' faces 100 either side of their centrelines.
    assert!(along(&faces, p(1.4, 4.9), p(8.6, 4.9)));
    assert!(along(&faces, p(1.6, 5.1), p(8.4, 5.1)));
    // The partition's faces 75 either side of x = 5.2, from the wall it
    // meets: never its centreline.
    assert!(along(&faces, p(5.125, 10.0), p(5.125, 14.0)));
    assert!(along(&faces, p(5.275, 10.0), p(5.275, 14.0)));
    for (a, b) in [
        ((5.2, 10.0), (5.2, 14.0)),
        ((5.2, 10.0), (8.5, 10.0)),
        ((1.5, 5.0), (8.5, 5.0)),
    ] {
        assert!(
            !along(&faces, p(a.0, a.1), p(b.0, b.1)),
            "a centreline {a:?}–{b:?}"
        );
    }
    // The doors on A-DOOR with their swings, the window's glass on A-WINDOW.
    let on = |name: &str| {
        let id = drawing.layers().find(name).unwrap();
        group
            .members
            .iter()
            .filter(|e| drawing.properties(**e).is_some_and(|p| p.layer == id))
            .count()
    };
    assert_eq!(
        on("A-DOOR"),
        2 * 5,
        "two leaves of four sides and a swing each"
    );
    assert_eq!(on("A-WINDOW"), 2);
}

#[test]
fn walls_that_cannot_be_are_refused_with_why() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm", "WALLS", "J"]);
    assert_eq!(h.prompt(), "Enter the walls as JSON:");
    h.enter_all(&[
        r#"{"walls": [{"from": [0, 0], "to": [1000, 0], "thickness": 200}], "openings": [{"wall": 0, "at": 900, "width": 400, "kind": "door"}]}"#,
        "",
    ]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Opening 0 is not wholly on its wall"),
        "{:?}",
        h.log()
    );
    assert_eq!(h.session.drawing().len(), 0);
}
