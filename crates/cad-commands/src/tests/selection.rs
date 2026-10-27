//! Click selection (DESIGN §16.1, §26.3 criterion 12).

use std::cell::RefCell;
use std::rc::Rc;

use cad_core::{Aabb, EntityId, Point2, ScreenPoint};
use cad_document::spatial::RTreeIndex;
use cad_document::{Drawing, SpatialIndex};

use crate::testing::Harness;
use crate::{Dirty, Modifiers, OverlayItem, Session};

/// Two horizontal lines, 50 units apart, drawn through the LINE command.
fn two_lines() -> (Harness, EntityId, EntityId) {
    let mut h = Harness::new();
    h.enter_all(&["L", "-100,0", "100,0", ""]);
    h.enter_all(&["L", "-100,50", "100,50", ""]);
    let ids = h.ids();
    (h, ids[0], ids[1])
}

fn selected(h: &Harness) -> Vec<EntityId> {
    h.session.selection().iter().collect()
}

#[test]
fn clicking_a_line_selects_it() {
    let (mut h, lower, upper) = two_lines();
    let dirty = h.click_world(Point2::new(20.0, 0.0));
    assert!(dirty.contains(Dirty::OVERLAY | Dirty::UI));
    assert_eq!(selected(&h), vec![lower]);
    assert_eq!(h.session.ui_state().selection_count, 1);

    // A second click adds to the selection, as in classic CAD.
    h.click_world(Point2::new(-60.0, 50.0));
    assert_eq!(selected(&h), vec![lower, upper]);
    // Clicking a selected entity again changes nothing.
    assert!(h.click_world(Point2::new(-60.0, 50.0)).is_empty());
}

#[test]
fn clicking_near_a_line_selects_it_within_the_pick_tolerance() {
    let (mut h, lower, _) = two_lines();
    h.click_world(Point2::new(0.0, 4.0));
    assert_eq!(selected(&h), vec![lower]);
}

#[test]
fn clicking_empty_space_keeps_the_selection_and_opens_a_box() {
    let (mut h, lower, _) = two_lines();
    h.click_world(Point2::new(0.0, 0.0));
    let dirty = h.click_world(Point2::new(0.0, 25.0));
    assert!(dirty.contains(Dirty::OVERLAY), "the box is shown");
    assert_eq!(selected(&h), vec![lower]);

    // Escape closes the box first and leaves the selection alone.
    h.escape();
    assert_eq!(selected(&h), vec![lower]);
    h.escape();
    assert!(selected(&h).is_empty());
}

#[test]
fn the_nearest_line_wins_and_ties_go_to_the_older_entity() {
    let mut h = Harness::new();
    h.enter_all(&["L", "-100,0", "100,0", ""]);
    h.enter_all(&["L", "-100,6", "100,6", ""]);
    let ids = h.ids();

    h.click_world(Point2::new(0.0, 4.0));
    assert_eq!(
        selected(&h),
        vec![ids[1]],
        "2 units from the upper line, 4 from the lower"
    );

    h.escape();
    h.click_world(Point2::new(0.0, 3.0));
    assert_eq!(selected(&h), vec![ids[0]], "equidistant: the older entity");
}

#[test]
fn the_pick_radius_is_constant_in_pixels_at_every_zoom() {
    for wheel in [0.0, -500.0, 500.0] {
        let (mut h, lower, _) = two_lines();
        h.session
            .wheel(ScreenPoint::new(400.0, 300.0), wheel, Modifiers::NONE);
        let on_line = h.session.view().world_to_screen(Point2::new(10.0, 0.0));

        h.click(on_line.x, on_line.y + 4.0);
        assert_eq!(selected(&h), vec![lower], "4px away at wheel {wheel}");

        h.escape();
        h.click(on_line.x, on_line.y + 7.0);
        assert!(selected(&h).is_empty(), "7px away at wheel {wheel}");
    }
}

#[test]
fn escape_clears_the_selection() {
    let (mut h, _, _) = two_lines();
    h.click_world(Point2::new(0.0, 0.0));
    let dirty = h.escape();
    assert!(dirty.contains(Dirty::OVERLAY | Dirty::UI));
    assert!(selected(&h).is_empty());
}

/// DESIGN §13.4: Escape×2 returns to idle and clears the selection.
#[test]
fn two_escapes_always_reach_idle_with_nothing_selected() {
    let (mut h, _, _) = two_lines();
    h.click_world(Point2::new(0.0, 0.0));
    h.enter_all(&["L", "0,0"]);
    h.escape();
    h.escape();
    assert_eq!(h.prompt(), "Command:");
    assert!(selected(&h).is_empty());
}

#[test]
fn selection_is_session_state_and_never_touches_the_document() {
    let (mut h, _, _) = two_lines();
    let revision = h.session.drawing().revision();
    let can_undo = h.session.ui_state().can_undo;

    let dirty = h.click_world(Point2::new(0.0, 0.0));
    assert!(!dirty.contains(Dirty::SCENE));
    h.escape();

    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.session.ui_state().can_undo, can_undo);
}

#[test]
fn selected_entities_are_described_as_highlights_then_grips_then_the_crosshair() {
    let (mut h, lower, _) = two_lines();
    h.click_world(Point2::new(30.0, 0.0));
    assert_eq!(
        h.session.overlay(),
        vec![
            OverlayItem::Highlight(lower),
            OverlayItem::Grips {
                at: vec![
                    Point2::new(-100.0, 0.0),
                    Point2::new(0.0, 0.0),
                    Point2::new(100.0, 0.0)
                ]
            },
            OverlayItem::Crosshair {
                at: Point2::new(30.0, 0.0)
            }
        ]
    );
}

#[test]
fn a_click_during_a_command_is_a_point_not_a_selection() {
    let (mut h, _, _) = two_lines();
    h.enter("L");
    h.click_world(Point2::new(0.0, 0.0));
    assert!(selected(&h).is_empty());
    assert_eq!(h.prompt(), "Specify next point or [Undo]:");
}

// ── criterion 12: selection uses an indexed candidate query ──

#[derive(Debug, Default)]
struct SpyLog {
    queries: Vec<Aabb>,
}

/// An index that records every query and can be told to hide everything.
#[derive(Clone, Debug)]
struct SpyIndex {
    inner: RTreeIndex,
    log: Rc<RefCell<SpyLog>>,
    hide_everything: bool,
}

impl SpatialIndex for SpyIndex {
    fn boxed_clone(&self) -> Box<dyn SpatialIndex> {
        Box::new(self.clone())
    }

    fn insert(&mut self, id: EntityId, bounds: Aabb) {
        self.inner.insert(id, bounds);
    }
    fn remove(&mut self, id: EntityId) {
        self.inner.remove(id);
    }
    fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        self.log.borrow_mut().queries.push(*bounds);
        if self.hide_everything {
            Vec::new()
        } else {
            self.inner.query(bounds)
        }
    }
    fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>) {
        self.inner.rebuild(entries);
    }
    fn len(&self) -> usize {
        self.inner.len()
    }
}

fn spied(hide_everything: bool) -> (Harness, Rc<RefCell<SpyLog>>) {
    let log = Rc::new(RefCell::new(SpyLog::default()));
    let index = SpyIndex {
        inner: RTreeIndex::new(),
        log: Rc::clone(&log),
        hide_everything,
    };
    let mut session = Session::with_drawing(Drawing::with_index(Box::new(index)));
    session.resize(800.0, 600.0);
    let mut h = Harness { session };
    for row in 0..200 {
        let y = f64::from(row) * 20.0 - 2000.0;
        h.enter_all(&["L", &format!("-100,{y}"), &format!("100,{y}"), ""]);
    }
    log.borrow_mut().queries.clear();
    (h, log)
}

#[test]
fn a_click_makes_one_index_query_sized_by_the_pick_tolerance() {
    let (mut h, log) = spied(false);
    h.click_world(Point2::new(0.0, 0.0));

    let log = log.borrow();
    assert_eq!(log.queries.len(), 1);
    let query = log.queries[0];
    // 5px on each side at one pixel per unit.
    assert_eq!(query.min(), Point2::new(-5.0, -5.0));
    assert_eq!(query.max(), Point2::new(5.0, 5.0));
    assert_eq!(selected(&h).len(), 1);
}

#[test]
fn only_index_candidates_are_tested_the_drawing_is_never_scanned() {
    // The line is right under the cursor, but the index proposes nothing.
    // A full scan would find it; an indexed pick must not.
    let (mut h, log) = spied(true);
    h.click_world(Point2::new(0.0, 0.0));

    assert!(selected(&h).is_empty());
    assert_eq!(log.borrow().queries.len(), 1);
}

/// A right click on an object at the idle prompt, with nothing selected,
/// selects it for the shortcut menu. With a selection, during a command, or
/// over nothing, it changes nothing: the menu acts on what is there.
#[test]
fn a_right_click_on_an_object_selects_it_when_nothing_is() {
    let (mut h, lower, upper) = two_lines();
    assert!(h.right_click_world(Point2::new(500.0, 500.0)).is_empty());
    assert!(selected(&h).is_empty());

    let dirty = h.right_click_world(Point2::new(20.0, 0.0));
    assert!(dirty.contains(Dirty::OVERLAY | Dirty::UI));
    assert_eq!(selected(&h), vec![lower]);
    // The selection stands: a right click elsewhere does not add to it.
    assert!(h.right_click_world(Point2::new(20.0, 50.0)).is_empty());
    assert_eq!(selected(&h), vec![lower]);

    h.escape();
    h.enter("MOVE");
    assert!(h.right_click_world(Point2::new(20.0, 50.0)).is_empty());
    assert!(!selected(&h).contains(&upper));
    let revision = h.session.drawing().revision();
    h.escape();
    assert_eq!(h.session.drawing().revision(), revision);
}

/// An object on a locked layer is not selected by a right click either.
#[test]
fn a_right_click_leaves_a_locked_object_alone() {
    let (mut h, _, _) = two_lines();
    h.enter_all(&["-LAYER", "Lock", "0", ""]);
    h.right_click_world(Point2::new(20.0, 0.0));
    assert!(selected(&h).is_empty());
}
