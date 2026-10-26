//! WINDOW and DOOR (#325): an opening through a wall already drawn — two
//! lines a drafter drew, or a floor plan's — cut through both faces,
//! closed by jambs, with the glass or the leaf and swing, as one undo;
//! and what is refused.

use cad_core::Point2;
use cad_document::{Entity, LineEntity};
use cad_layout::{Room, Setbacks, Spec, Terrain};

use crate::floorplan::{DOORS, EXTERIOR_WALLS, WINDOWS};
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A wall 0.2 thick along the x axis from 0 to 10: its two faces.
fn wall() -> Harness {
    Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into(),
        LineEntity::new(p(10.0, 0.2), p(0.0, 0.2)).into(),
    ])
}

fn on(h: &Harness, layer: &str) -> Vec<Entity> {
    let drawing = h.session.drawing();
    let Some(id) = drawing.layers().find(layer) else {
        return Vec::new();
    };
    drawing
        .entities()
        .filter(|(e, _)| drawing.properties(*e).is_some_and(|p| p.layer == id))
        .map(|(_, e)| e.clone())
        .collect()
}

fn lines_on_layer_zero(h: &Harness) -> Vec<LineEntity> {
    on(h, "0")
        .into_iter()
        .filter_map(|e| match e {
            Entity::Line(line) => Some(line),
            _ => None,
        })
        .collect()
}

#[test]
fn a_window_cuts_both_faces_closes_them_with_jambs_and_draws_its_glass_as_one_undo() {
    let mut h = wall();
    h.enter("WINDOW");
    assert_eq!(h.prompt(), "Select a face of the wall:");
    h.click_world(p(4.0, 0.0));
    assert_eq!(h.prompt(), "Specify the window's centre on the wall:");
    h.enter("4,0.1");
    assert_eq!(h.prompt(), "Specify window width <1.2>:");
    let revision = h.session.drawing().revision();
    h.enter("1.5");
    assert_eq!(h.prompt(), "Command:");
    assert_ne!(h.session.drawing().revision(), revision);

    // Each face in two pieces, and a jamb at each end, on the wall's layer.
    let lines = lines_on_layer_zero(&h);
    assert_eq!(lines.len(), 6);
    let xs = |line: &LineEntity| (line.start.x.min(line.end.x), line.start.x.max(line.end.x));
    let mut pieces: Vec<(f64, f64)> = lines
        .iter()
        .filter(|l| (l.start.y - l.end.y).abs() < 1e-9)
        .map(xs)
        .collect();
    pieces.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(pieces.len(), 4);
    assert!((pieces[0].1 - 3.25).abs() < 1e-9 && (pieces[2].0 - 4.75).abs() < 1e-9);
    let jambs: Vec<&LineEntity> = lines
        .iter()
        .filter(|l| (l.start.x - l.end.x).abs() < 1e-9)
        .collect();
    assert_eq!(jambs.len(), 2);
    assert!(
        jambs
            .iter()
            .all(|j| (j.start.y - j.end.y).abs() - 0.2 < 1e-9)
    );
    // The glass: two lines along the opening on WINDOW.
    let glass = on(&h, WINDOWS);
    assert_eq!(glass.len(), 2);
    for entity in &glass {
        let line = entity.as_line().unwrap();
        assert!((line.start.distance_to(line.end) - 1.5).abs() < 1e-9);
        assert!(line.start.y > 0.0 && line.start.y < 0.2);
    }
    // One undo: the two faces whole again.
    h.undo();
    assert_eq!(lines_on_layer_zero(&h).len(), 2);
    assert!(on(&h, WINDOWS).is_empty());
}

#[test]
fn a_door_asks_its_side_and_draws_a_leaf_and_a_swing() {
    let mut h = wall();
    h.enter("DOOR");
    h.click_world(p(4.0, 0.2));
    h.enter("4,0.1");
    assert_eq!(h.prompt(), "Specify door width <0.9>:");
    h.enter("");
    assert_eq!(
        h.prompt(),
        "Specify a point on the side the door opens into:"
    );
    h.enter("3,2");
    assert_eq!(h.prompt(), "Command:");
    let door = on(&h, DOORS);
    assert_eq!(door.len(), 2);
    let leaf = door
        .iter()
        .find_map(|e| match e {
            Entity::Polyline(polyline) => Some(polyline),
            _ => None,
        })
        .expect("a leaf");
    assert!(leaf.closed && leaf.vertices.len() == 4);
    // Hinged at the end nearer 3,2 — the left — on the upper face, standing
    // into the room above, 5 cm thick.
    assert!((leaf.vertices[0].x - 3.55).abs() < 1e-9 && (leaf.vertices[0].y - 0.2).abs() < 1e-9);
    assert!((leaf.vertices[1].y - 1.1).abs() < 1e-9);
    assert!((leaf.vertices[2].x - 3.5).abs() < 1e-9);
    let arc = door
        .iter()
        .find_map(|e| match e {
            Entity::Arc(arc) => Some(arc),
            _ => None,
        })
        .expect("a swing");
    assert!((arc.radius - 0.9).abs() < 1e-9 && arc.center.distance_to(leaf.vertices[0]) < 1e-9);
    assert_eq!(lines_on_layer_zero(&h).len(), 6);
    h.undo();
    assert!(on(&h, DOORS).is_empty());
    assert_eq!(lines_on_layer_zero(&h).len(), 2);
}

#[test]
fn what_is_not_a_wall_and_an_opening_that_does_not_fit_are_refused() {
    // A lone line has no face across from it.
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into()]);
    h.enter("WINDOW");
    h.click_world(p(5.0, 0.0));
    assert_eq!(
        h.log().last().unwrap(),
        "No face lies across from that one: a wall is two parallel faces."
    );
    assert_eq!(h.prompt(), "Select a face of the wall:");
    h.session.key(crate::KeyAction::Escape);

    let mut h = wall();
    h.enter("WINDOW");
    h.click_world(p(4.0, 0.0));
    // Off the wall: asked again.
    h.enter("4,3");
    assert_eq!(
        h.log().last().unwrap(),
        "Put the window's centre on the wall."
    );
    assert_eq!(h.prompt(), "Specify the window's centre on the wall:");
    // Too near the end for the width: asked again, told what fits.
    h.enter("0.5,0.1");
    h.enter("1.5");
    assert_eq!(
        h.log().last().unwrap(),
        "Only 1 fits there: move the window away from the wall's end, or make it narrower."
    );
    assert_eq!(h.prompt(), "Specify window width <1.2>:");
    h.enter("0.8");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(on(&h, WINDOWS).len(), 2);
    // A circle is not a face.
    let mut h = Harness::with_entities([cad_document::CircleEntity::new(p(0.0, 0.0), 5.0).into()]);
    h.enter("DOOR");
    h.click_world(p(5.0, 0.0));
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Select a straight face of a wall")
    );
}

/// A plan's exterior wall is a polyline (its outer face) and a line (the
/// room's side): a window cuts the polyline into pieces as it does lines.
#[test]
fn a_window_goes_through_a_generated_plans_exterior_wall() {
    let spec = Spec {
        terrain: Terrain {
            width: 12.0,
            depth: 20.0,
        },
        setbacks: Setbacks::default(),
        rooms: vec![
            Room::new("living", 25.0),
            Room::new("kitchen", 10.0),
            Room::new("bedroom", 12.0),
        ],
        adjacent: Vec::new(),
        exterior_wall: 0.25,
        interior_wall: 0.15,
        door: 0.9,
        max_circulation: 0.15,
        windows: None,
        seed: 0,
    };
    let layout = cad_layout::solve(&spec).unwrap();
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &spec.to_json(), "", ""]);
    assert!(h.log().last().unwrap().starts_with("Plan 1 drawn"));
    let outer_before = on(&h, EXTERIOR_WALLS).len();
    // The middle of the bedroom's longest outside wall, on the outer face.
    let room = &layout.rooms[2];
    let tolerance = cad_core::tolerance::LinearTolerance::DEFAULT.at_magnitude(20.0);
    let sides = room.rect.outside_sides(&layout.outline, tolerance);
    let (face, centre) = if sides[2] > 0.0 {
        let x = room.inner.x + room.inner.w / 2.0;
        (p(x, layout.footprint.y), p(x, room.rect.y))
    } else if sides[3] > 0.0 {
        let x = room.inner.x + room.inner.w / 2.0;
        (p(x, layout.footprint.top()), p(x, room.rect.top()))
    } else if sides[0] > 0.0 {
        let y = room.inner.y + room.inner.h / 2.0;
        (p(layout.footprint.x, y), p(room.rect.x, y))
    } else {
        let y = room.inner.y + room.inner.h / 2.0;
        (p(layout.footprint.right(), y), p(room.rect.right(), y))
    };
    h.enter("WINDOW");
    h.click_world(face);
    assert_eq!(h.prompt(), "Specify the window's centre on the wall:");
    h.enter(&format!("{},{}", centre.x, centre.y));
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(on(&h, WINDOWS).len(), 2);
    // The outer face gained a piece, the room's side was cut in two, and
    // two jambs were added: four more on WALL-EXT.
    assert_eq!(on(&h, EXTERIOR_WALLS).len(), outer_before + 4);
    h.undo();
    assert_eq!(on(&h, EXTERIOR_WALLS).len(), outer_before);
}

/// A door through a plan's interior wall: its faces are two rooms' sides,
/// on WALL-INT; the leaf stands into the room the point is in.
#[test]
fn a_door_goes_through_a_generated_plans_interior_wall() {
    let spec = Spec {
        terrain: Terrain {
            width: 12.0,
            depth: 20.0,
        },
        setbacks: Setbacks::default(),
        rooms: vec![
            Room::new("living", 25.0),
            Room::new("kitchen", 10.0),
            Room::new("bedroom", 12.0),
        ],
        adjacent: Vec::new(),
        exterior_wall: 0.25,
        interior_wall: 0.15,
        door: 0.9,
        max_circulation: 0.15,
        windows: None,
        seed: 0,
    };
    let layout = cad_layout::solve(&spec).unwrap();
    let walls = cad_layout::walls(&spec, &layout);
    // The longest interior face: its middle is clear of the plan's doors
    // on one side or the other, and the face across is the next room's.
    let face = walls
        .interior
        .iter()
        .max_by(|a, b| {
            a.start
                .distance_to(a.end)
                .total_cmp(&b.start.distance_to(b.end))
        })
        .unwrap();
    assert!(face.start.distance_to(face.end) > 1.5);
    let middle = Point2::new(
        (face.start.x + face.end.x) / 2.0,
        (face.start.y + face.end.y) / 2.0,
    );
    let room = layout.rooms[face.room].inner;
    let inside = Point2::new(room.x + room.w / 2.0, room.y + room.h / 2.0);
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &spec.to_json(), "", ""]);
    let interior_before = on(&h, crate::floorplan::INTERIOR_WALLS).len();
    let doors_before = on(&h, DOORS).len();
    h.enter("DOOR");
    h.click_world(middle);
    assert_eq!(
        h.prompt(),
        "Specify the door's centre on the wall:",
        "{:?}",
        h.log()
    );
    h.enter(&format!("{},{}", middle.x, middle.y));
    h.enter("0.8");
    h.enter(&format!("{},{}", inside.x, inside.y));
    assert_eq!(h.prompt(), "Command:", "{:?}", h.log());
    assert_eq!(on(&h, DOORS).len(), doors_before + 2);
    // Both faces cut in two, and two jambs: four more on WALL-INT.
    assert_eq!(
        on(&h, crate::floorplan::INTERIOR_WALLS).len(),
        interior_before + 4
    );
    // The leaf stands into the room the point is in.
    let leaf = on(&h, DOORS)
        .into_iter()
        .filter_map(|e| match e {
            Entity::Polyline(p) => Some(p),
            _ => None,
        })
        .last()
        .unwrap();
    let tip = leaf.vertices[1];
    assert!(
        tip.x >= room.x - 1e-9
            && tip.x <= room.right() + 1e-9
            && tip.y >= room.y - 1e-9
            && tip.y <= room.top() + 1e-9,
        "{tip:?} in {room:?}"
    );
    h.undo();
    assert_eq!(on(&h, DOORS).len(), doors_before);
}
