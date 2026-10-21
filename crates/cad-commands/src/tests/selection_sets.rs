//! Selection sets: multiple selection, Shift to remove, window and crossing
//! boxes (DESIGN §16.1).

use cad_core::{Aabb, EntityId, Point2};
use cad_document::spatial::RTreeIndex;
use cad_document::{Drawing, SpatialIndex};

use crate::testing::Harness;
use crate::{Dirty, Modifiers, OverlayItem, PointerButton, Session};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A line from (-100,0) to (100,0), a circle of radius 30 at (0,80), and a
/// rectangle from (150,-20) to (250,60).
fn scene() -> (Harness, EntityId, EntityId, EntityId) {
    let mut h = Harness::new();
    h.enter_all(&["L", "-100,0", "100,0", ""]);
    h.enter_all(&["C", "0,80", "30"]);
    h.enter_all(&["REC", "150,-20", "250,60"]);
    let ids = h.ids();
    (h, ids[0], ids[1], ids[2])
}

fn selected(h: &Harness) -> Vec<EntityId> {
    h.session.selection().iter().collect()
}

#[test]
fn shift_click_removes_an_entity_from_the_selection() {
    let (mut h, line, circle, _) = scene();
    h.click_world(p(0.0, 0.0));
    h.click_world(p(30.0, 80.0));
    assert_eq!(selected(&h), vec![line, circle]);

    let dirty = h.shift_click_world(p(0.0, 0.0));
    assert!(dirty.contains(Dirty::OVERLAY | Dirty::UI));
    assert_eq!(selected(&h), vec![circle]);

    // Shift-clicking something that is not selected changes nothing.
    assert!(h.shift_click_world(p(0.0, 0.0)).is_empty());
}

#[test]
fn a_window_box_left_to_right_selects_only_what_is_entirely_inside() {
    let (mut h, line, circle, rectangle) = scene();
    // Covers the whole line and the whole circle, but only half the rectangle.
    h.select_box(p(-120.0, -30.0), p(200.0, 120.0));
    assert_eq!(selected(&h), vec![line, circle]);
    assert!(!selected(&h).contains(&rectangle));
}

#[test]
fn a_crossing_box_right_to_left_selects_everything_it_touches() {
    let (mut h, line, circle, rectangle) = scene();
    // The same rectangle, drawn the other way.
    h.select_box(p(200.0, 120.0), p(-120.0, -30.0));
    assert_eq!(selected(&h), vec![line, circle, rectangle]);
}

#[test]
fn a_crossing_box_inside_a_circle_or_rectangle_touches_nothing() {
    let (mut h, ..) = scene();
    h.select_box(p(5.0, 85.0), p(-5.0, 75.0)); // inside the circle
    h.select_box(p(210.0, 30.0), p(190.0, 10.0)); // inside the rectangle
    assert!(selected(&h).is_empty(), "interiors are not geometry");
}

#[test]
fn a_crossing_box_over_one_edge_selects_the_whole_polyline() {
    let (mut h, _, _, rectangle) = scene();
    h.select_box(p(260.0, 30.0), p(240.0, 10.0)); // straddles the right edge only
    assert_eq!(selected(&h), vec![rectangle]);
}

#[test]
fn boxes_add_to_the_selection_and_shift_boxes_remove() {
    let (mut h, line, circle, rectangle) = scene();
    h.click_world(p(0.0, 0.0));
    h.select_box(p(300.0, 100.0), p(140.0, -40.0)); // crossing over the rectangle
    assert_eq!(selected(&h), vec![line, rectangle]);

    // Shift + crossing box over the line and circle removes what it finds.
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let from = h.session.view().world_to_screen(p(120.0, 120.0));
    let to = h.session.view().world_to_screen(p(-120.0, -30.0));
    h.session
        .pointer_down(from, PointerButton::Primary, Modifiers::NONE);
    h.session.pointer_down(to, PointerButton::Primary, shift);
    assert_eq!(selected(&h), vec![rectangle]);
    assert!(!selected(&h).contains(&circle));
}

#[test]
fn press_drag_release_also_draws_a_box() {
    let (mut h, line, circle, _) = scene();
    let from = h.session.view().world_to_screen(p(-120.0, -30.0));
    let to = h.session.view().world_to_screen(p(120.0, 120.0));
    h.session
        .pointer_down(from, PointerButton::Primary, Modifiers::NONE);
    h.session.pointer_move(to, Modifiers::NONE);
    h.session
        .pointer_up(to, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(selected(&h), vec![line, circle]);

    // A click without movement leaves the box open for a second click.
    let (mut h, ..) = scene();
    h.click(10.0, 10.0);
    assert!(
        h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::SelectionBox { .. }))
    );
}

#[test]
fn the_box_is_described_in_the_overlay_with_its_kind() {
    let (mut h, ..) = scene();
    h.click_world(p(-10.0, 30.0));
    h.move_to(450.0, 250.0); // world (50, 50): to the right, a window
    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::SelectionBox {
        from: p(-10.0, 30.0),
        to: p(50.0, 50.0),
        crossing: false
    }));

    h.move_to(350.0, 250.0); // world (-50, 50): to the left, crossing
    let overlay = h.session.overlay();
    assert!(overlay.contains(&OverlayItem::SelectionBox {
        from: p(-10.0, 30.0),
        to: p(-50.0, 50.0),
        crossing: true
    }));
}

#[test]
fn selecting_never_changes_the_document() {
    let (mut h, ..) = scene();
    let revision = h.session.drawing().revision();
    h.select_box(p(300.0, 200.0), p(-200.0, -200.0));
    h.shift_click_world(p(0.0, 0.0));
    h.escape();
    assert_eq!(h.session.drawing().revision(), revision);
}

/// Candidates come from the spatial index: one query per box, sized by the box.
#[test]
fn a_box_selection_makes_one_index_query_for_the_box() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug)]
    struct Spy(RTreeIndex, Rc<RefCell<Vec<Aabb>>>);
    impl SpatialIndex for Spy {
        fn boxed_clone(&self) -> Box<dyn SpatialIndex> {
            Box::new(self.clone())
        }

        fn insert(&mut self, id: EntityId, bounds: Aabb) {
            self.0.insert(id, bounds);
        }
        fn remove(&mut self, id: EntityId) {
            self.0.remove(id);
        }
        fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
            self.1.borrow_mut().push(*bounds);
            self.0.query(bounds)
        }
        fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>) {
            self.0.rebuild(entries);
        }
        fn len(&self) -> usize {
            self.0.len()
        }
    }

    let queries = Rc::new(RefCell::new(Vec::new()));
    let index = Spy(RTreeIndex::new(), Rc::clone(&queries));
    let mut session = Session::with_drawing(Drawing::with_index(Box::new(index)));
    session.resize(800.0, 600.0);
    let mut h = Harness { session };
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.click_world(p(-50.0, -50.0)); // empty: one pick query, then the box opens
    queries.borrow_mut().clear();

    h.click_world(p(50.0, 50.0));
    assert_eq!(
        *queries.borrow(),
        vec![Aabb::from_points(p(-50.0, -50.0), p(50.0, 50.0))]
    );
    assert_eq!(h.session.selection().len(), 1);
}
