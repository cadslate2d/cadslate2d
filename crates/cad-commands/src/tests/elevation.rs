//! ELEVATION (#337): a drawn plan's façades, by a pick or the plan's group
//! name, each side, the brief's roof or another, one undo, in a group of
//! its own; what is not a plan refused.

use cad_core::Point2;
use cad_document::Entity;
use cad_layout::{Side, elevation};

use crate::floorplan::drawn_plan;
use crate::testing::Harness;

const BRIEF: &str = r#"{"terrain": {"width": 15, "depth": 20}, "setbacks": {"front": 3},
    "rooms": [{"name": "living", "area": 24, "entrance": true}, {"name": "kitchen", "area": 10},
    {"name": "bedroom", "area": 12}], "adjacent": [["living", "kitchen"]], "windows": 1.2}"#;

/// A drawing in millimetres with the brief's plan drawn at the origin.
fn planned() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&[
        "UNITS",
        "mm",
        "FLOORPLAN",
        "B",
        &BRIEF.replace('\n', " "),
        "",
        "",
    ]);
    assert!(
        h.log().last().unwrap().starts_with("Plan 1 drawn"),
        "{:?}",
        h.log()
    );
    h
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

#[test]
fn a_front_elevation_by_the_plans_name_is_drawn_to_scale_as_one_undo() {
    let mut h = planned();
    let before = h.entities().len();
    h.enter("ELEVATION");
    assert_eq!(h.prompt(), "Select an object of a floor plan or [Name]:");
    h.enter_all(&["N", "PLAN-1"]);
    assert_eq!(h.prompt(), "Which side? [Front/Back/Left/Right] <Front>:");
    h.enter_all(&["", ""]);
    assert_eq!(
        h.prompt(),
        "Specify the left end of the façade on the ground <0,-12000>:"
    );
    h.enter("0,-12000");
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("Front elevation of PLAN-1 drawn as ELEV-1:"),
        "{said}"
    );
    // The wall outline: the footprint's width in millimetres, 3.5 m high
    // to the parapet.
    let plan = drawn_plan(h.session.drawing(), "PLAN-1").unwrap();
    let width = plan.layout.footprint.w * 1000.0;
    let walls = on(&h, "ELEV-WALL");
    let Some(Entity::Polyline(outline)) = walls.first() else {
        panic!("{walls:?}");
    };
    let xs: Vec<f64> = outline.vertices.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = outline.vertices.iter().map(|p| p.y).collect();
    let span = |v: &[f64]| {
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    assert!((span(&xs) - width).abs() < 1e-6);
    assert!((span(&ys) - 3500.0).abs() < 1e-6);
    assert!(
        outline
            .vertices
            .iter()
            .any(|p| p.distance_to(Point2::new(0.0, -12000.0)) < 1e-6)
    );
    // The front's openings: its windows and the way in.
    let wanted = elevation(&plan.spec, &plan.layout, Side::Front)
        .openings
        .len();
    assert_eq!(on(&h, "ELEV-OPENING").len(), wanted);
    assert!(wanted >= 2);
    // A group of its own, saying what it was drawn from.
    let drawing = h.session.drawing();
    let group = drawing
        .groups()
        .iter()
        .map(|(_, g)| g)
        .find(|g| g.name == "ELEV-1")
        .expect("its group");
    assert!(
        group.description.contains("\"plan\":\"PLAN-1\""),
        "{}",
        group.description
    );
    assert!(group.description.contains("\"side\":\"front\""));
    h.undo();
    assert_eq!(h.entities().len(), before);
}

#[test]
fn a_side_picked_from_the_plan_with_a_gable_roof_at_another_pitch() {
    let mut h = planned();
    let plan = drawn_plan(h.session.drawing(), "PLAN-1").unwrap();
    // A point on the building's outer face: its lower left corner.
    let fp = plan.layout.footprint;
    let corner = Point2::new(fp.x * 1000.0, fp.y * 1000.0);
    h.enter("ELEVATION");
    h.click_world(corner);
    assert_eq!(h.prompt(), "Which side? [Front/Back/Left/Right] <Front>:");
    h.enter_all(&["L", "G"]);
    assert_eq!(h.prompt(), "Specify roof pitch <30>:");
    h.enter_all(&["35", "30000,0"]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Left elevation of PLAN-1"),
        "{:?}",
        h.log()
    );
    assert!(!on(&h, "ELEV-ROOF").is_empty());
    // The roof drawn is the pitch asked: as the façade worked out with it.
    let mut spec = plan.spec.clone();
    spec.building.roof = cad_layout::Roof::Gable;
    spec.building.roof_pitch = 35.0;
    let facade = elevation(&spec, &plan.layout, Side::Left);
    let top = on(&h, "ELEV-ROOF")
        .iter()
        .filter_map(|e| match e {
            Entity::Polyline(p) => p.vertices.iter().map(|v| v.y).reduce(f64::max),
            _ => None,
        })
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((top - facade.height * 1000.0).abs() < 1e-6, "{top}");
}

#[test]
fn what_is_not_a_plan_is_refused_with_the_reason() {
    let mut h = planned();
    h.enter_all(&["LINE", "-5000,-5000", "-4000,-5000", ""]);
    h.enter("ELEVATION");
    h.click_world(Point2::new(-4500.0, -5000.0));
    assert_eq!(
        h.log().last().unwrap(),
        "That object is not part of a floor plan drawn by FLOORPLAN."
    );
    h.enter_all(&["N", "PLAN-9"]);
    assert_eq!(h.log().last().unwrap(), "No group is called PLAN-9.");
    assert_eq!(h.prompt(), "Select an object of a floor plan or [Name]:");
}
