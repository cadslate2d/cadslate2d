//! FLOORPLAN (#281): briefs at the command line, as JSON, and from a
//! file, to drawings; the plans previewed and cycled; the one accepted
//! drawn as one undo; briefs with no plan refused with the reasons.

use cad_core::Point2;
use cad_document::Entity;
use cad_layout::{Room, Setbacks, Spec, Terrain};

use crate::floorplan::{DOORS, EXTERIOR_WALLS, INTERIOR_WALLS, ROOM_TEXT};
use crate::stair::STAIRS;
use crate::testing::Harness;
use crate::{HostRequest, OverlayItem};

fn on(h: &Harness, layer: &str) -> usize {
    let drawing = h.session.drawing();
    let Some(id) = drawing.layers().find(layer) else {
        return 0;
    };
    drawing
        .entities()
        .filter(|(e, _)| drawing.properties(*e).is_some_and(|p| p.layer == id))
        .count()
}

fn texts(h: &Harness) -> Vec<String> {
    h.entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Text(t) => Some(t.content),
            _ => None,
        })
        .collect()
}

fn house() -> Spec {
    Spec {
        terrain: Terrain {
            width: 15.0,
            depth: 20.0,
        },
        setbacks: Setbacks {
            front: 3.0,
            ..Setbacks::default()
        },
        rooms: vec![
            Room::new("living room", 24.0),
            Room::new("kitchen", 10.0),
            Room::new("bedroom", 12.0),
            Room::new("bath", 4.5),
        ],
        adjacent: vec![["living room".into(), "kitchen".into()]],
        exterior_wall: 0.25,
        interior_wall: 0.15,
        door: 0.9,
        max_circulation: 0.15,
        windows: None,
        storey_height: 2.8,
        building: cad_layout::Building::default(),
        seed: 0,
    }
}

#[test]
fn a_brief_asked_for_is_planned_previewed_cycled_and_drawn_as_one_undo() {
    let mut h = Harness::new();
    h.enter("FLOORPLAN");
    assert_eq!(h.prompt(), "Specify terrain width or [Brief/File] <15>:");
    h.enter("15");
    assert_eq!(h.prompt(), "Specify terrain depth <20>:");
    h.enter("20");
    assert_eq!(h.prompt(), "Enter a room's name:");
    // A name may have spaces in it.
    h.enter_all(&[
        "living room",
        "24",
        "kitchen",
        "10",
        "bedroom",
        "12",
        "bath",
        "4.5",
    ]);
    // A name twice is refused.
    h.enter("kitchen");
    assert_eq!(h.log().last().unwrap(), "A room is already called kitchen.");
    h.enter("");
    assert_eq!(
        h.prompt(),
        "Enter two rooms that open onto each other as name/name, or press Enter:"
    );
    h.enter("living room/garage");
    assert_eq!(
        h.log().last().unwrap(),
        "Name two different rooms of the brief, as kitchen/living."
    );
    h.enter_all(&["living room/kitchen", ""]);
    assert_eq!(
        h.prompt(),
        "Specify the windows' width, or press Enter for no windows:"
    );
    h.enter("0");
    assert_eq!(
        h.log().last().unwrap(),
        "A window's width must be greater than zero."
    );
    h.enter("");
    assert_eq!(
        h.prompt(),
        "Enter the room that holds the stair, or press Enter for none:"
    );
    h.enter("garage");
    assert_eq!(
        h.log().last().unwrap(),
        "No room of the brief is called garage."
    );
    h.enter("");
    assert_eq!(h.prompt(), "Specify the terrain's lower left corner <0,0>:");
    let revision = h.session.drawing().revision();
    h.enter("");
    h.move_to_world(Point2::new(1.0, 1.0));
    assert!(
        h.log().iter().any(|l| l.ends_with("found. Plan 1 of 3.")),
        "{:?}",
        h.log()
    );
    assert_eq!(
        h.prompt(),
        "Plan 1 of 3 or [Next/Previous/Accept] <Accept>:"
    );
    // Previewed, not drawn.
    let previewed = h
        .session
        .overlay()
        .into_iter()
        .filter(|item| matches!(item, OverlayItem::PreviewEntity(_)))
        .count();
    assert!(previewed > 20, "{previewed}");
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter("N");
    assert_eq!(
        h.prompt(),
        "Plan 2 of 3 or [Next/Previous/Accept] <Accept>:"
    );
    h.enter("P");
    h.enter("P");
    assert_eq!(
        h.prompt(),
        "Plan 3 of 3 or [Next/Previous/Accept] <Accept>:"
    );
    h.enter("");
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Plan 3 drawn: 4 rooms,"),
        "{:?}",
        h.log()
    );
    assert_eq!(h.prompt(), "Command:");
    assert!(on(&h, EXTERIOR_WALLS) > 4);
    assert!(on(&h, INTERIOR_WALLS) > 4);
    assert!(
        on(&h, DOORS) >= 8,
        "a leaf and a swing for each of four doors at least"
    );
    let labels = texts(&h);
    for name in ["living room", "kitchen", "bedroom", "bath"] {
        assert!(labels.contains(&name.to_owned()), "{labels:?}");
    }
    assert!(labels.iter().any(|t| t.ends_with(" m²")));
    assert_eq!(on(&h, ROOM_TEXT), labels.len());
    h.undo();
    assert!(h.entities().is_empty());
}

#[test]
fn a_brief_as_json_or_from_a_file_and_where_the_terrain_goes() {
    let json = house().to_json();
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B"]);
    assert_eq!(h.prompt(), "Enter the brief as JSON:");
    h.enter("{not a brief");
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("The brief cannot be read:")
    );
    h.enter_all(&[&json, "100,50", ""]);
    assert!(
        h.log().last().unwrap().starts_with("Plan 1 drawn"),
        "{:?}",
        h.log()
    );
    // The terrain's corner at 100,50: the building is beyond its setback.
    let lowest = h
        .entities()
        .iter()
        .filter_map(|e| e.as_line().map(|l| l.start.y.min(l.end.y)))
        .fold(f64::INFINITY, f64::min);
    assert!(lowest >= 53.0, "{lowest}");

    // File: the host is asked for one, and types its text in.
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "F"]);
    assert_eq!(h.session.take_host_request(), Some(HostRequest::OpenBrief));
    assert_eq!(h.prompt(), "Enter the brief as JSON:");
    h.enter_all(&[&json, "", ""]);
    assert!(
        h.log().last().unwrap().starts_with("Plan 1 drawn"),
        "{:?}",
        h.log()
    );
}

#[test]
fn a_drawing_in_millimetres_takes_lengths_in_millimetres() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    h.enter_all(&[
        "FLOORPLAN",
        "12000",
        "10000",
        "office",
        "20",
        "store",
        "6",
        "",
        "",
        // Windows 1.5 m wide, typed in the drawing's millimetres (#324).
        "1500",
        "",
        "",
        "",
    ]);
    assert!(
        h.log().last().unwrap().starts_with("Plan 1 drawn"),
        "{:?}",
        h.log()
    );
    let widest = h
        .entities()
        .iter()
        .filter_map(|e| e.as_line().map(|l| l.start.x.max(l.end.x)))
        .fold(0.0, f64::max);
    assert!(widest > 1000.0 && widest <= 12000.0, "{widest}");
    let drawing = h.session.drawing();
    let window = drawing.layers().find("WINDOW").expect("made");
    let glass: Vec<f64> = drawing
        .entities()
        .filter(|(id, _)| drawing.properties(*id).is_some_and(|p| p.layer == window))
        .filter_map(|(_, entity)| entity.as_line().map(|l| l.start.distance_to(l.end)))
        .collect();
    assert!(!glass.is_empty());
    assert!(
        glass
            .iter()
            .all(|&length| length > 500.0 && length <= 1500.0 + 1e-6),
        "{glass:?}"
    );
}

#[test]
fn a_brief_with_no_plan_is_refused_as_impossible_or_as_not_found() {
    let mut h = Harness::new();
    let revision = h.session.drawing().revision();
    h.enter_all(&[
        "FLOORPLAN",
        "8",
        "8",
        "living",
        "40",
        "bedroom",
        "30",
        "",
        "",
        "",
        "",
        "",
    ]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("This cannot be built: The rooms need at least 66.5 m²"),
        "{:?}",
        h.log()
    );
    assert_eq!(h.prompt(), "Command:");
    // Possible, perhaps, but not without circulation, which it forbids.
    let mut spec = Spec {
        rooms: vec![
            Room {
                entrance: true,
                ..Room::new("living", 25.0)
            },
            Room::new("bath", 4.0),
        ],
        adjacent: vec![["living".into(), "bath".into()]],
        max_circulation: 0.0,
        ..house()
    };
    spec.terrain = Terrain {
        width: 10.0,
        depth: 12.0,
    };
    spec.setbacks = Setbacks::default();
    h.enter_all(&["FLOORPLAN", "B", &spec.to_json(), ""]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("No plan was found in 40 searches, though one may exist"),
        "{:?}",
        h.log()
    );
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn the_preview_is_the_plan_drawn() {
    let json = house().to_json();
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &json, ""]);
    h.move_to_world(Point2::new(1.0, 1.0));
    let mut previewed: Vec<Entity> = h
        .session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewEntity(e) => Some(e),
            _ => None,
        })
        .collect();
    h.enter("");
    let mut drawn = h.entities();
    let key = |e: &Entity| format!("{e:?}");
    previewed.sort_by_key(key);
    drawn.sort_by_key(key);
    assert_eq!(previewed, drawn);
}

/// The house with a hall that holds the stair (#331).
fn house_with_a_stair() -> Spec {
    let mut spec = house();
    let mut hall = Room::new("hall", 12.0);
    hall.stair = true;
    spec.rooms.push(hall);
    spec.adjacent.push(["hall".into(), "living room".into()]);
    spec
}

#[test]
fn a_room_given_the_stair_has_it_drawn_inside_it_as_part_of_the_one_undo() {
    let spec = house_with_a_stair();
    // The plan the search makes of the brief with the hall sized for a stair.
    let plan = &cad_layout::alternatives(&crate::floorplan::searched(&spec), 3).unwrap()[0];
    let inner = plan.rooms[4].inner;
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &spec.to_json(), "", ""]);
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("Plan 1 drawn") && !said.contains("No stair"),
        "{said}"
    );
    // The storey of 2.8 m climbs in 16 risers, written up the first
    // flight: square to the plan, in radians.
    let said = h
        .entities()
        .into_iter()
        .find_map(|e| match e {
            Entity::Text(t) if t.content == "16 R × 17.5 / T 28.0" => Some(t),
            _ => None,
        })
        .expect("the stair says its risers");
    let quarters = said.rotation / std::f64::consts::FRAC_PI_2;
    assert!((quarters - quarters.round()).abs() < 1e-9 && quarters.abs() <= 4.0);
    assert!(on(&h, STAIRS) > 15, "{}", on(&h, STAIRS));
    // Every point of the stair is inside the hall's walls.
    let drawing = h.session.drawing();
    let layer = drawing.layers().find(STAIRS).unwrap();
    let points: Vec<Point2> = drawing
        .entities()
        .filter(|(e, _)| drawing.properties(*e).is_some_and(|p| p.layer == layer))
        .flat_map(|(_, entity)| match entity {
            Entity::Line(l) => vec![l.start, l.end],
            Entity::Polyline(p) => p.vertices.clone(),
            _ => vec![],
        })
        .collect();
    assert!(!points.is_empty());
    for q in points {
        assert!(
            q.x >= inner.x - 1e-9
                && q.x <= inner.right() + 1e-9
                && q.y >= inner.y - 1e-9
                && q.y <= inner.top() + 1e-9,
            "{q:?} outside {inner:?}"
        );
    }
    h.enter("U");
    assert_eq!(on(&h, STAIRS), 0);
    assert!(h.entities().is_empty());
}

#[test]
fn a_stair_asked_for_at_the_command_line_climbs_the_storey_in_a_room_made_for_it() {
    let typed = |stair_room: &str| {
        let mut h = Harness::new();
        h.enter_all(&["UNITS", "mm"]);
        h.enter_all(&[
            "FLOORPLAN",
            "12000",
            "10000",
            "office",
            "20",
            "store",
            "6",
            "",
            "",
            "",
            stair_room,
        ]);
        h
    };
    let mut h = typed("store");
    assert_eq!(h.prompt(), "Specify floor-to-floor height <2800>:");
    h.enter("0");
    assert_eq!(
        h.log().last().unwrap(),
        "The storey's height must be greater than zero."
    );
    // 3 m climbs in 18 risers of 16.7 and treads of 29.7: a U, a passage
    // beside it, and the floor at its foot take 2.7 × 4.17 m, more than the
    // store's 6 m².
    h.enter_all(&["3000", ""]);
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("This cannot be built: The store's 6 m² cannot be 2.7 m wide both ways.")
            && said.ends_with("The room that holds the stair needs 2.70 × 4.17 m."),
        "{said}"
    );
    assert!(h.session.drawing().layers().find(STAIRS).is_none());

    let mut h = typed("office");
    h.enter_all(&["3000", "", ""]);
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("Plan 1 drawn") && !said.contains("No stair"),
        "{said}"
    );
    assert!(texts(&h).contains(&"18 R × 16.7 / T 29.7".to_owned()));
    assert!(on(&h, STAIRS) > 15);
}

/// A stair hall that is the way in too has a door on every wall, and a stair
/// against any of them would stand in a doorway (#344). It is sized for a
/// passage either side of the stair: too small for that, it is refused
/// saying what it needs; large enough, its stair is drawn.
#[test]
fn a_stair_hall_that_is_the_way_in_is_sized_for_a_passage_either_side() {
    let brief = |area: f64| {
        format!(
            r#"{{"terrain":{{"width":12,"depth":15}},"rooms":[{{"name":"living room","area":22}},{{"name":"kitchen","area":9}},{{"name":"stair hall","area":{area},"entrance":true,"stair":true}},{{"name":"toilet","area":3}}],"adjacent":[["living room","kitchen"],["living room","stair hall"],["stair hall","toilet"]],"storey_height":3}}"#
        )
    };
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &brief(12.0), ""]);
    let said = h
        .log()
        .into_iter()
        .find(|line| line.starts_with("This cannot be built"))
        .unwrap_or_else(|| panic!("{:?}", h.log()));
    assert!(
        said.starts_with("This cannot be built: The stair hall's 12 m² cannot be 3.6 m wide")
            && said.ends_with(
                "The room that holds the stair and the way in needs 3.60 × 4.17 m: a passage either side of the stair."
            ),
        "{said}"
    );
    assert!(h.entities().is_empty());

    // Every plan of a hall that can hold it has its stair.
    let spec = Spec::from_json(&brief(16.0)).unwrap();
    for plan in crate::floorplan::plans(&spec).unwrap() {
        assert_eq!(crate::floorplan::stair_misfits(&spec, &plan), None);
    }
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &brief(16.0), "", ""]);
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("Plan 1 drawn") && !said.contains("No stair"),
        "{said}"
    );
    assert!(on(&h, STAIRS) > 15);
}
