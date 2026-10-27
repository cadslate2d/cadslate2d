//! PARTVIEWS (#339): a part's views from its JSON or a drawn front view,
//! in either projection, sectioned or not, agreeing with each other, as
//! one undo; what cannot be a part refused with the reason.

use cad_core::Point2;
use cad_document::Entity;

use crate::testing::Harness;

const BLOCK: &str = r#"{"profile": [[0, 0], [100, 0], [100, 50], [0, 50]], "depth": 40, "holes": [{"axis": "y", "at": [60, 25], "diameter": 10}]}"#;

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

fn lines(entities: &[Entity]) -> Vec<(Point2, Point2)> {
    entities
        .iter()
        .filter_map(|e| e.as_line().map(|l| (l.start, l.end)))
        .collect()
}

#[test]
fn a_prisms_views_agree_and_are_drawn_first_angle_as_one_undo() {
    let mut h = Harness::new();
    h.enter("PARTVIEWS");
    assert_eq!(h.prompt(), "Describe the part or [Json/Select] <Json>:");
    h.enter_all(&["", BLOCK]);
    assert_eq!(h.prompt(), "Which projection? [First/Third] <First>:");
    h.enter_all(&["", ""]);
    assert_eq!(
        h.prompt(),
        "Specify the front view's lower left corner <0,0>:"
    );
    h.enter("0,0");
    assert_eq!(
        h.log().last().unwrap(),
        "Part views drawn as PART-1: front, top, and right side, first-angle. LTSCALE set to 10 for its hidden and centre lines."
    );
    // The hole: a circle in front at (60, 25); its sides hidden in the top
    // view, under the front, at x 55 and 65; in the side view, left of the
    // front, at heights 20 and 30.
    let circles: Vec<(Point2, f64)> = on(&h, "PART-VISIBLE")
        .iter()
        .filter_map(|e| match e {
            Entity::Circle(c) => Some((c.center, c.radius)),
            _ => None,
        })
        .collect();
    assert_eq!(circles, [(Point2::new(60.0, 25.0), 5.0)]);
    let hidden = lines(&on(&h, "PART-HIDDEN"));
    let top: Vec<f64> = hidden
        .iter()
        .filter(|(a, b)| a.y < 0.0 && (a.x - b.x).abs() < 1e-9)
        .map(|(a, _)| a.x)
        .collect();
    assert_eq!(top, [55.0, 65.0]);
    let side: Vec<f64> = hidden
        .iter()
        .filter(|(a, b)| a.x < 0.0 && (a.y - b.y).abs() < 1e-9)
        .map(|(a, _)| a.y)
        .collect();
    assert_eq!(side, [20.0, 30.0]);
    // Hidden lines in the HIDDEN linetype, through the layer.
    let drawing = h.session.drawing();
    let layer = drawing
        .layers()
        .get(drawing.layers().find("PART-HIDDEN").unwrap())
        .unwrap();
    assert!(matches!(layer.linetype, cad_document::Linetype::Named(_)));
    assert_eq!(drawing.ltscale(), 10.0);
    h.undo();
    assert!(h.entities().is_empty());
    assert_eq!(h.session.drawing().ltscale(), 1.0);
}

#[test]
fn third_angle_puts_the_top_view_above_and_the_side_on_the_right() {
    let mut h = Harness::new();
    h.enter_all(&["PARTVIEWS", "", BLOCK, "T", "N", "0,0"]);
    let visible = lines(&on(&h, "PART-VISIBLE"));
    assert!(visible.iter().any(|(a, _)| a.y > 50.0), "a top view above");
    assert!(
        visible.iter().any(|(a, _)| a.x > 100.0),
        "a side view on the right"
    );
    assert!(visible.iter().all(|(a, _)| a.y >= 0.0 && a.x >= 0.0));
}

#[test]
fn a_section_is_hatched_and_its_cutting_plane_marked_in_the_top_view() {
    let mut h = Harness::new();
    h.enter_all(&["PARTVIEWS", "", BLOCK, "", "Y", "0,0"]);
    let hatches = on(&h, "PART-HATCH");
    assert_eq!(hatches.len(), 1);
    let texts: Vec<String> = on(&h, "PART-TEXT")
        .iter()
        .filter_map(|e| match e {
            Entity::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"SECTION A-A".to_owned()), "{texts:?}");
    assert_eq!(texts.iter().filter(|t| *t == "A").count(), 2);
    // The cutting plane: halfway back in the top view, under the front.
    let cut = lines(&on(&h, "PART-CUT"));
    assert_eq!(cut.len(), 1);
    assert!(cut[0].0.y < 0.0 && (cut[0].0.y - cut[0].1.y).abs() < 1e-9);
    // No hidden lines in the section, the top and side keep theirs.
    assert!(
        lines(&on(&h, "PART-HIDDEN"))
            .iter()
            .all(|(a, _)| a.y < 0.0 || a.x < 0.0)
    );
}

#[test]
fn a_turned_part_has_a_front_and_a_side_and_a_drawn_front_view_its_depth() {
    let mut h = Harness::new();
    h.enter_all(&[
        "PARTVIEWS",
        "",
        r#"{"steps": [[40, 30], [60, 20], [30, 50]], "bore": 10}"#,
        "",
        "",
        "0,0",
    ]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Part views drawn as PART-1: front and right side, first-angle.")
    );
    // A front view drawn by hand: its outline and a hole, given a depth.
    let mut h = Harness::new();
    h.enter_all(&["RECTANG", "0,0", "80,40", "CIRCLE", "20,20", "6"]);
    h.enter_all(&["PARTVIEWS", "S"]);
    assert!(
        h.prompt()
            .starts_with("Select the front view's outline and its holes or [")
    );
    h.enter_all(&["ALL", ""]);
    assert_eq!(h.prompt(), "Specify the part's depth:");
    h.enter_all(&["25", "", "", "200,0"]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("Part views drawn as PART-1"),
        "{:?}",
        h.log()
    );
    let hidden = lines(&on(&h, "PART-HIDDEN"));
    // The hole's sides in the top view, under the new front at x 200.
    assert!(hidden.iter().any(|(a, _)| (a.x - 214.0).abs() < 1e-9));
}

#[test]
fn what_cannot_be_a_part_is_refused_and_asked_again() {
    let mut h = Harness::new();
    h.enter_all(&["PARTVIEWS", ""]);
    h.enter("{not json");
    assert!(
        h.log()
            .last()
            .unwrap()
            .starts_with("The part cannot be read:")
    );
    h.enter(r#"{"profile": [[0, 0], [10, 0], [10, 10]], "depth": 5, "holes": [{"axis": "y", "at": [50, 50], "diameter": 2}]}"#);
    assert_eq!(h.log().last().unwrap(), "Hole 1 is not inside the part.");
    assert_eq!(h.prompt(), "Enter the part as JSON:");
    h.enter(r#"{"steps": [[10, 5]], "bore": 12}"#);
    assert_eq!(
        h.log().last().unwrap(),
        "The bore must be narrower than every step."
    );
}
