//! Edits keep constraints (#270, ADR-0038 §5): every modify command and
//! every grip edit on constrained geometry is settled in its own
//! transaction — the rest of the part follows, as one undo — or refused,
//! saying which constraints it would break. After any accepted edit, every
//! constraint holds.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, LineEntity};
use proptest::prelude::*;

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn id(n: u64) -> EntityId {
    EntityId::from_raw(n)
}

fn line(h: &Harness, n: u64) -> LineEntity {
    *h.session
        .drawing()
        .entity(id(n))
        .and_then(Entity::as_line)
        .expect("a line")
}

/// A 100 × 50 plate of four lines — bottom 1, right 2, top 3, left 4 —
/// its corners coincident, its sides level and plumb; its lower left
/// corner fixed when `fixed`, its width dimensioned d1 = 100 when
/// `measured`. Its height is free.
fn plate(fixed: bool, measured: bool) -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", "100,50", "0,50", "C"]);
    for corner in ["100,0", "100,50", "0,50", "0,0"] {
        h.enter_all(&["GCCOINCIDENT", corner, corner]);
    }
    for (kind, at) in [
        ("GCHORIZONTAL", p(50.0, 0.0)),
        ("GCHORIZONTAL", p(50.0, 50.0)),
        ("GCVERTICAL", p(100.0, 25.0)),
        ("GCVERTICAL", p(0.0, 25.0)),
    ] {
        h.enter(kind);
        h.click_world(at);
    }
    if fixed {
        h.enter_all(&["GCFIX", "0,0"]);
    }
    if measured {
        h.enter_all(&["DCHORIZONTAL", "0,0", "100,0", "50,-15", ""]);
    }
    let expected = 8 + usize::from(fixed) + usize::from(measured);
    assert_eq!(
        h.session.drawing().constraints().len(),
        expected,
        "{:?}",
        h.log()
    );
    assert!(h.session.drawing().unsatisfied_constraints().is_empty());
    h
}

/// Every constraint holds, and the plate is still a rectangle.
fn holds(h: &Harness) {
    assert!(
        h.session.drawing().unsatisfied_constraints().is_empty(),
        "{:?}",
        h.session.drawing().unsatisfied_constraints()
    );
}

fn refused(h: &Harness) -> bool {
    h.log()
        .last()
        .is_some_and(|line| line.starts_with("The edit would break"))
}

#[test]
fn move_takes_the_part_along_and_a_fix_refuses_it() {
    let mut h = plate(true, true);
    // The top line up 10: the sides lengthen, the bottom stays.
    h.enter("MOVE");
    h.click_world(p(50.0, 50.0));
    h.enter_all(&["", "0,0", "0,10"]);
    holds(&h);
    assert_eq!(line(&h, 3).start, p(100.0, 60.0));
    assert!((line(&h, 2).end.y - 60.0).abs() < 1e-7);
    assert!((line(&h, 4).start.y - 60.0).abs() < 1e-7);
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));
    // One undo takes back the move and what followed it.
    h.undo();
    assert!((line(&h, 2).end.y - 50.0).abs() < 1e-9);

    // The bottom line moved: the fix refuses it, and nothing changes.
    let revision = h.session.drawing().revision();
    h.enter("MOVE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "5,5"]);
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("fix"), "{:?}", h.log());
    assert_eq!(h.session.drawing().revision(), revision);

    // Unfixed, the whole plate moves freely.
    let mut h = plate(false, true);
    h.enter_all(&["MOVE", "ALL", "", "0,0", "30,-20"]);
    holds(&h);
    assert_eq!(line(&h, 1).start, p(30.0, -20.0));
}

#[test]
fn rotate_and_scale_are_refused_where_they_break_a_constraint() {
    let mut h = plate(true, true);
    // Turning the top line breaks its level.
    let revision = h.session.drawing().revision();
    h.enter("ROTATE");
    h.click_world(p(50.0, 50.0));
    h.enter_all(&["", "50,50", "10"]);
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("horizontal"));
    assert_eq!(h.session.drawing().revision(), revision);

    // Scaling the plate breaks d1.
    h.enter_all(&["SCALE", "ALL", "", "0,0", "2"]);
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("d1"));

    // Without a width or a fix, a plate scales and turns whole.
    let mut h = plate(false, false);
    h.enter_all(&["SCALE", "ALL", "", "0,0", "2"]);
    holds(&h);
    assert_eq!(line(&h, 2).end, p(200.0, 100.0));
    h.enter_all(&["ROTATE", "ALL", "", "0,0", "90"]);
    assert!(refused(&h), "a quarter turn makes the level plumb");
}

#[test]
fn a_grip_drags_the_part_with_it_and_its_preview_shows_it() {
    let mut h = plate(true, true);
    // The right side's top end, dragged up to 80: the top line follows,
    // level, and the left side lengthens.
    h.click_world(p(100.0, 25.0));
    h.click_world(p(100.0, 50.0));
    h.move_to_world(p(100.0, 80.0));
    // While dragged, the preview shows the top line where it would settle.
    let previews: Vec<Entity> = h
        .session
        .drawing()
        .settled_with(&[(id(2), LineEntity::new(p(100.0, 0.0), p(100.0, 80.0)).into())])
        .unwrap()
        .into_iter()
        .map(|(_, entity)| entity)
        .collect();
    assert!(previews.iter().any(
        |entity| matches!(entity, Entity::Line(l) if (l.start.y - 80.0).abs() < 1e-7 && (l.end.y - 80.0).abs() < 1e-7)
    ));
    h.click_world(p(100.0, 80.0));
    holds(&h);
    assert!((line(&h, 3).end.y - 80.0).abs() < 1e-7);
    assert!((line(&h, 4).start.y - 80.0).abs() < 1e-7);

    // Its bottom end dragged right: d1 refuses it.
    h.escape();
    h.click_world(p(100.0, 25.0));
    h.click_world(p(100.0, 0.0));
    h.click_world(p(130.0, 0.0));
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("d1"));
    assert_eq!(line(&h, 2).start, p(100.0, 0.0));
}

#[test]
fn stretch_trim_fillet_erase_and_offset_on_a_constrained_plate() {
    let mut h = plate(true, true);
    // A crossing window over the top edge, stretched up 20.
    h.enter_all(&["STRETCH", "110,60", "-10,40", "0,0", "0,20"]);
    holds(&h);
    assert!((line(&h, 3).start.y - 70.0).abs() < 1e-7);

    // Filleting a constrained corner cuts its lines back from the corner
    // their coincidence holds: refused.
    h.enter_all(&["FILLET", "R", "5"]);
    h.click_world(p(50.0, 0.0));
    h.click_world(p(100.0, 30.0));
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("coincident"));
    h.escape();

    // An offset is a new object, not constrained: the plate stays.
    h.enter_all(&["OFFSET", "10"]);
    h.click_world(p(50.0, 70.0));
    h.click_world(p(50.0, 100.0));
    h.escape();
    holds(&h);
    assert_eq!(h.session.drawing().len(), 5);

    // Erasing a side takes its constraints; the rest still hold.
    let before = h.session.drawing().constraints().len();
    h.enter("ERASE");
    h.click_world(p(0.0, 30.0));
    h.enter("");
    holds(&h);
    assert_eq!(h.session.drawing().constraints().len(), before - 3);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// After any accepted edit — a line moved, a grip dragged, the plate
    /// stretched — every constraint holds within tolerance; a refused one
    /// leaves the drawing as it was.
    #[test]
    fn after_any_accepted_edit_every_constraint_holds(
        edits in prop::collection::vec((0..3u8, 1..5u64, -40.0..40.0f64, -40.0..40.0f64), 1..6),
        fixed in any::<bool>(),
        measured in any::<bool>(),
    ) {
        let mut h = plate(fixed, measured);
        for (kind, which, dx, dy) in edits {
            let revision = h.session.drawing().revision();
            let before = h.entities();
            let l = line(&h, which);
            let middle = p((l.start.x + l.end.x) / 2.0, (l.start.y + l.end.y) / 2.0);
            match kind {
                0 => {
                    h.enter("MOVE");
                    h.click_world(middle);
                    h.enter_all(&["", "0,0", &format!("{dx},{dy}")]);
                }
                1 => {
                    h.escape();
                    h.escape();
                    h.click_world(middle);
                    h.click_world(l.end);
                    h.click_world(p(l.end.x + dx, l.end.y + dy));
                    h.escape();
                    h.escape();
                }
                _ => {
                    let corner = l.end;
                    h.enter_all(&[
                        "STRETCH",
                        &format!("{},{}", corner.x + 3.0, corner.y + 3.0),
                        &format!("{},{}", corner.x - 3.0, corner.y - 3.0),
                        "0,0",
                        &format!("{dx},{dy}"),
                    ]);
                }
            }
            prop_assert!(
                h.session.drawing().unsatisfied_constraints().is_empty(),
                "{:?}",
                h.log()
            );
            if h.log().last().is_some_and(|line| line.starts_with("The edit would break")) {
                prop_assert_eq!(h.session.drawing().revision(), revision);
                prop_assert_eq!(h.entities(), before);
            }
            h.escape();
        }
    }
}
