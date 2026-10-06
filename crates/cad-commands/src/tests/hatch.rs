//! HATCH through the session (#167, ADR-0021): a region found from a
//! point, or from selected objects; its refusals; the preview; what the
//! modify commands do with a hatch. The fill itself is tested in
//! `cad-document` and `cad-geometry`.

use std::f64::consts::PI;

use cad_core::Point2;
use cad_document::{
    ArcEntity, CircleEntity, Entity, GripKind, HatchEntity, LineEntity, PolylineEntity,
};

use crate::OverlayItem;
use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn hatches(h: &Harness) -> Vec<HatchEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .collect()
}

fn previewed(h: &Harness) -> Option<HatchEntity> {
    h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Hatch(hatch)) => Some(hatch),
        _ => None,
    })
}

/// A 200 by 100 rectangle about the origin with a circle of radius 20 in it.
fn plate() -> Vec<Entity> {
    vec![
        PolylineEntity::new(
            vec![
                p(-100.0, -50.0),
                p(100.0, -50.0),
                p(100.0, 50.0),
                p(-100.0, 50.0),
            ],
            true,
        )
        .into(),
        CircleEntity::new(p(0.0, 0.0), 20.0).into(),
    ]
}

#[test]
fn debug_loops() {
    let entities = plate();
    let refs: Vec<&Entity> = entities.iter().collect();
    let loops = crate::commands::hatch::loops_of(&refs);
    println!("loops: {}", loops.len());
    for path in &loops {
        let ring = cad_geometry::flatten_path(path, std::f64::consts::TAU / 64.0);
        println!(
            "ring {} points, first {:?}, area {}, contains(-80,0) {}, contains(-20,0) {}",
            ring.len(),
            ring.first(),
            cad_geometry::signed_area(&ring),
            cad_geometry::ring_contains(&ring, p(-80.0, 0.0)),
            cad_geometry::ring_contains(&ring, p(-20.0, 0.0))
        );
    }
}

#[test]
fn a_point_inside_finds_the_smallest_loop_around_it_and_the_loops_within() {
    let mut h = Harness::with_entities(plate());
    h.enter("H");
    assert_eq!(h.prompt(), "Pick internal point or [Select]:");
    h.enter("-80,0");
    assert_eq!(h.prompt(), "Command:");
    let all = hatches(&h);
    let [hatch] = all.as_slice() else {
        panic!("one hatch");
    };
    assert_eq!(hatch.loops.len(), 2, "the rectangle and the hole");
    let expected = 20_000.0 - PI * 400.0;
    assert!((hatch.area() - expected).abs() < 5.0, "{}", hatch.area());
    assert!(hatch.contains(p(-80.0, 0.0)) && !hatch.contains(p(0.0, 0.0)));
    assert_eq!(
        h.properties()[2].layer,
        h.session.drawing().settings().current_layer
    );

    // Inside the circle: the circle alone.
    h.enter_all(&["HATCH", "5,5"]);
    let inner = &hatches(&h)[1];
    assert_eq!(inner.loops.len(), 1);
    assert!((inner.area() - PI * 400.0).abs() < 5.0);

    // Outside everything: nothing bounds the point.
    h.enter_all(&["H", "300,0"]);
    assert_eq!(last(&h), "No closed boundary surrounds that point.");
    assert_eq!(h.prompt(), "Pick internal point or [Select]:");
    h.escape();

    h.undo();
    h.undo();
    assert!(hatches(&h).is_empty(), "one undo each");
}

#[test]
fn lines_and_arcs_that_meet_end_to_end_bound_a_region() {
    let mut h = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        LineEntity::new(p(100.0, 0.0), p(100.0, 60.0)).into(),
        // A rounded top: an arc from (100, 60) over to (0, 60).
        ArcEntity::new(p(50.0, 60.0), 50.0, 0.0, PI).into(),
        LineEntity::new(p(0.0, 60.0), p(0.0, 0.0)).into(),
        // A chain that does not close, off to the side.
        LineEntity::new(p(200.0, 0.0), p(300.0, 0.0)).into(),
        LineEntity::new(p(300.0, 0.0), p(300.0, 50.0)).into(),
    ]);
    h.enter_all(&["H", "50,30"]);
    let all = hatches(&h);
    let [hatch] = all.as_slice() else {
        panic!("one hatch");
    };
    assert_eq!(hatch.loops.len(), 1);
    let expected = 6000.0 + PI * 2500.0 / 2.0;
    assert!((hatch.area() - expected).abs() < 20.0, "{}", hatch.area());
    assert!(hatch.contains(p(50.0, 100.0)), "under the arc");
    // The open chain bounds nothing.
    h.enter_all(&["H", "250,20"]);
    assert_eq!(last(&h), "No closed boundary surrounds that point.");
    h.escape();
}

#[test]
fn select_takes_every_loop_the_selected_objects_make() {
    let mut h = Harness::with_entities(plate());
    h.enter_all(&["H", "S"]);
    assert_eq!(h.prompt(), "Select boundary objects:");
    h.select_box(p(-150.0, -80.0), p(150.0, 80.0));
    h.enter("");
    let all = hatches(&h);
    let [hatch] = all.as_slice() else {
        panic!("one hatch");
    };
    assert_eq!(hatch.loops.len(), 2);
    assert!(!hatch.contains(p(0.0, 0.0)), "the circle is a hole");
    assert!(h.session.selection().is_empty());

    // Nothing selected, or a boundary that does not close.
    h.enter_all(&["H", "S", ""]);
    assert_eq!(last(&h), "No objects selected.");
    assert_eq!(h.prompt(), "Command:");
    let mut open = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        LineEntity::new(p(100.0, 0.0), p(100.0, 60.0)).into(),
    ]);
    open.enter_all(&["H", "S"]);
    open.select_box(p(-10.0, -10.0), p(110.0, 70.0));
    open.enter("");
    assert_eq!(last(&open), "The boundary does not close.");
    assert!(hatches(&open).is_empty());
}

#[test]
fn the_preview_is_the_hatch_the_click_makes_and_touches_nothing() {
    let mut h = Harness::with_entities(plate());
    let revision = h.session.drawing().revision();
    h.enter("H");
    h.move_to_world(p(-80.0, 0.0));
    let preview = previewed(&h).expect("the fill at the pointer");
    assert_eq!(preview.loops.len(), 2);
    assert_eq!(h.session.drawing().revision(), revision);
    h.move_to_world(p(300.0, 0.0));
    assert!(previewed(&h).is_none(), "nothing to fill there");
    h.move_to_world(p(-80.0, 0.0));
    h.click_world(p(-80.0, 0.0));
    assert_eq!(hatches(&h), vec![preview]);
    assert_ne!(h.session.drawing().revision(), revision);
}

#[test]
fn a_hatch_is_picked_by_its_fill_moved_whole_and_refused_by_the_curve_commands() {
    let mut h = Harness::with_entities(plate());
    h.enter_all(&["H", "-80,0"]);
    h.click_world(p(-80.0, 0.0));
    assert_eq!(h.session.selection().len(), 1, "by its fill");
    let summary = h.session.selection_summary();
    assert_eq!(summary.kinds, [("hatch".to_owned(), 1)]);
    assert!(
        summary
            .geometry
            .contains(&("Loops".to_owned(), "2".to_owned()))
    );
    let kinds: Vec<GripKind> = h.session.grips().iter().map(|grip| grip.kind).collect();
    assert_eq!(kinds.len(), 6, "four corners and the circle's two vertices");
    assert!(kinds.iter().all(|kind| *kind == GripKind::Vertex));

    h.enter_all(&["M", "0,0", "10,20"]);
    let moved = &hatches(&h)[0];
    assert!(moved.contains(p(-70.0, 20.0)) && !moved.contains(p(10.0, 20.0)));
    h.click_world(p(-70.0, 20.0));
    h.enter_all(&["MI", "0,0", "0,10", ""]);
    assert_eq!(hatches(&h).len(), 2, "a mirrored copy");
    assert!((hatches(&h)[1].area() - hatches(&h)[0].area()).abs() < 1e-6);

    h.enter_all(&["O", "5"]);
    h.click_world(p(-70.0, 20.0));
    h.click_world(p(-70.0, 30.0));
    assert_eq!(last(&h), "A hatch cannot be offset.");
    h.escape();
    h.click_world(p(-70.0, 20.0));
    h.enter("X");
    assert_eq!(last(&h), "A hatch cannot be exploded.");
    assert_eq!(hatches(&h).len(), 2);
}
