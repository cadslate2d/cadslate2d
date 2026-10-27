//! BSECTION (#338): a section through a drawn plan, the walls it crosses
//! cut and hatched, the stair in section where the cut runs up it, the cut
//! marked on the plan with its letter, one undo; a cut that misses the
//! building refused.

use cad_core::Point2;
use cad_document::Entity;

use crate::floorplan::{drawn_plan, stairs};
use crate::testing::Harness;

const BRIEF: &str = r#"{"terrain": {"width": 15, "depth": 20}, "windows": 1.2,
    "rooms": [{"name": "living", "area": 24, "entrance": true}, {"name": "kitchen", "area": 10},
    {"name": "hall", "area": 12, "stair": true}], "adjacent": [["living", "kitchen"], ["hall", "living"]]}"#;

fn planned() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["FLOORPLAN", "B", &BRIEF.replace('\n', " "), "", ""]);
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
fn a_cut_up_the_stair_shows_it_climbing_the_storey_and_is_marked_on_the_plan() {
    let mut h = planned();
    let plan = drawn_plan(h.session.drawing(), "PLAN-1").unwrap();
    let placed = stairs(&plan.spec, &plan.layout)[0]
        .1
        .expect("the hall holds the stair");
    let fp = plan.layout.footprint;
    // The stair climbs along y or x; the cut runs up it through its foot.
    let cos = placed.turn.cos();
    let along_y = cos.abs() > 0.5;
    let (a, b) = if along_y {
        (
            Point2::new(placed.foot.x, fp.y - 1.0),
            Point2::new(placed.foot.x, fp.top() + 1.0),
        )
    } else {
        (
            Point2::new(fp.x - 1.0, placed.foot.y),
            Point2::new(fp.right() + 1.0, placed.foot.y),
        )
    };
    let before = h.entities().len();
    h.enter("BSECTION");
    h.enter_all(&["N", "PLAN-1"]);
    assert_eq!(h.prompt(), "Specify the cut's first point:");
    h.enter(&format!("{},{}", a.x, a.y));
    h.enter(&format!("{},{}", b.x, b.y));
    assert_eq!(h.prompt(), "Specify the left end of the floor <0,-12>:");
    h.enter("0,-12");
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("Section A-A of PLAN-1 drawn as SECT-1:"),
        "{said}"
    );
    // The stair's profile and underside, climbing from the floor.
    let stair = on(&h, "SECT-STAIR");
    assert_eq!(stair.len(), 2, "{stair:?}");
    let Entity::Polyline(profile) = &stair[0] else {
        panic!()
    };
    let top = profile
        .vertices
        .iter()
        .map(|v| v.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(top > -12.0 + 1.0, "{top}");
    // Cut walls outlined and hatched alike.
    let hatches = on(&h, "SECT-HATCH");
    assert!(hatches.len() >= 4);
    assert_eq!(
        hatches.len(),
        on(&h, "SECT-CUT")
            .iter()
            .filter(|e| matches!(e, Entity::Polyline(_)))
            .count()
    );
    // On the plan: the line, two arrows, two letters A.
    let marks = on(&h, "SECT-MARK");
    let letters: Vec<String> = marks
        .iter()
        .filter_map(|e| match e {
            Entity::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(letters, ["A", "A"]);
    h.undo();
    assert_eq!(h.entities().len(), before);
}

#[test]
fn the_second_section_is_b_and_a_cut_that_misses_is_refused() {
    let mut h = planned();
    let plan = drawn_plan(h.session.drawing(), "PLAN-1").unwrap();
    let fp = plan.layout.footprint;
    let middle = fp.y + fp.h / 2.0;
    h.enter_all(&["BSECTION", "N", "PLAN-1"]);
    h.enter(&format!("{},{}", fp.x - 1.0, middle));
    h.enter(&format!("{},{}", fp.right() + 1.0, middle));
    h.enter("");
    // Beside the building: refused, and asked again.
    h.enter_all(&["BSECTION", "N", "PLAN-1", "-5,-5", "-5,-1"]);
    assert_eq!(h.log().last().unwrap(), "The cut misses the building.");
    assert_eq!(h.prompt(), "Specify the cut's first point:");
    let x = fp.x + fp.w / 3.0;
    h.enter(&format!("{x},{}", fp.top() + 1.0));
    h.enter(&format!("{x},{}", fp.y - 1.0));
    h.enter("40,-12");
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Section B-B of PLAN-1 drawn as SECT-2"),
        "{:?}",
        h.log()
    );
}
