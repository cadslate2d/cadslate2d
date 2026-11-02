//! The pointer, through the same session API the browser uses: a point
//! the host gives that is not on the screen — not a number, or infinite —
//! is no event at all (#403).

use cad_core::{Point2, ScreenPoint};
use cad_document::{Entity, LineEntity};

use crate::testing::Harness;
use crate::{Dirty, Modifiers, PointerButton};

const NOWHERE: [ScreenPoint; 3] = [
    ScreenPoint::new(f64::NAN, f64::NAN),
    ScreenPoint::new(f64::INFINITY, 300.0),
    ScreenPoint::new(400.0, f64::NEG_INFINITY),
];

#[test]
fn a_click_that_is_not_on_the_screen_is_no_point() {
    for nowhere in NOWHERE {
        let mut h = Harness::new();
        h.enter("L");
        let prompt = h.session.ui_state().prompt;
        let primary = PointerButton::Primary;
        assert_eq!(
            h.session.pointer_move(nowhere, Modifiers::NONE),
            Dirty::NONE
        );
        assert_eq!(
            h.session.pointer_down(nowhere, primary, Modifiers::NONE),
            Dirty::NONE
        );
        assert_eq!(
            h.session.pointer_up(nowhere, primary, Modifiers::NONE),
            Dirty::NONE
        );
        assert_eq!(h.session.ui_state().prompt, prompt, "still the first point");
        h.click_world(Point2::new(0.0, 0.0));
        h.click_world(Point2::new(10.0, 0.0));
        h.enter("");
        let lines: Vec<Entity> = h
            .session
            .drawing()
            .entities()
            .map(|(_, e)| e.clone())
            .collect();
        assert_eq!(
            lines,
            [Entity::from(LineEntity::new(
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0)
            ))],
            "{nowhere:?}"
        );
    }
}

#[test]
fn the_view_does_not_follow_a_pointer_that_is_not_on_the_screen() {
    for nowhere in NOWHERE {
        let mut h = Harness::new();
        let view = h.session.view().clone();
        assert_eq!(
            h.session.wheel(nowhere, -120.0, Modifiers::NONE),
            Dirty::NONE
        );
        // A middle drag from nowhere, or to it, pans nothing.
        let middle = PointerButton::Middle;
        assert_eq!(
            h.session.pointer_down(nowhere, middle, Modifiers::NONE),
            Dirty::NONE
        );
        h.session
            .pointer_move(ScreenPoint::new(100.0, 100.0), Modifiers::NONE);
        h.session
            .pointer_down(ScreenPoint::new(100.0, 100.0), middle, Modifiers::NONE);
        h.session.pointer_move(nowhere, Modifiers::NONE);
        h.session.pointer_up(nowhere, middle, Modifiers::NONE);
        assert_eq!(h.session.view(), &view, "{nowhere:?}");
        assert_eq!(h.session.double_click(nowhere), Dirty::NONE);
    }
}

/// A view given a size that is not finite keeps the size it had, and a
/// cursor no view could map to a finite point is nowhere: an ordinary click
/// never places a point that is not one (#403).
#[test]
fn a_size_that_is_not_finite_leaves_the_view_as_it_was() {
    for (width, height) in [
        (f64::INFINITY, 600.0),
        (800.0, f64::NAN),
        (f64::NEG_INFINITY, f64::INFINITY),
    ] {
        let mut h = Harness::new();
        let view = h.session.view().clone();
        h.session.resize(width, height);
        assert_eq!(h.session.view(), &view, "{width} × {height}");
        h.enter("L");
        for x in [400.0, 500.0] {
            let at = ScreenPoint::new(x, 300.0);
            h.session
                .pointer_down(at, PointerButton::Primary, Modifiers::NONE);
            h.session
                .pointer_up(at, PointerButton::Primary, Modifiers::NONE);
        }
        h.enter("");
        let ends: Vec<Point2> = h
            .session
            .drawing()
            .entities()
            .flat_map(|(_, entity)| match entity {
                Entity::Line(line) => vec![line.start, line.end],
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(ends.len(), 2, "one line");
        assert!(ends.iter().all(|point| point.is_finite()), "{ends:?}");
    }
}
