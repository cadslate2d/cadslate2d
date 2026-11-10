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
        .settled_with(
            &[(id(2), LineEntity::new(p(100.0, 0.0), p(100.0, 80.0)).into())],
            &[],
        )
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

#[test]
fn scale_and_rotate_about_a_constrained_point_do_not_slide_the_line() {
    // #420: the base point keeps its bits, and was left free to slide.
    let measured = || {
        let mut h = Harness::new();
        h.enter_all(&["LINE", "0,0", "100,0", ""]);
        h.enter_all(&["DCHORIZONTAL", "0,0", "100,0", "50,-15", ""]);
        assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
        h
    };
    let mut h = measured();
    let revision = h.session.drawing().revision();
    h.enter_all(&["SCALE", "ALL", "", "0,0", "2"]);
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("d1"), "{:?}", h.log());
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));
    assert_eq!(line(&h, 1).end, p(100.0, 0.0));

    // A half turn reverses a horizontal distance: refused, not moved 200.
    let mut h = measured();
    h.enter_all(&["ROTATE", "ALL", "", "0,0", "180"]);
    assert!(refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));

    // An aligned length turns rigidly about its own end.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["DCALIGNED", "0,0", "100,0", "50,-15", ""]);
    h.enter_all(&["ROTATE", "ALL", "", "0,0", "90"]);
    holds(&h);
    assert!(!refused(&h), "{:?}", h.log());
    let turned = line(&h, 1);
    assert_eq!(turned.start, p(0.0, 0.0));
    assert!((turned.end - p(0.0, 100.0)).length() < 1e-9, "{turned:?}");

    // What is joined to a turned line but not turned still follows it.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "100,0", "100,50", ""]);
    h.enter_all(&["GCCOINCIDENT", "100,0", "100,0"]);
    h.enter("ROTATE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "90"]);
    holds(&h);
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));
    assert!(
        (line(&h, 2).start - p(0.0, 100.0)).length() < 1e-9,
        "{:?}",
        line(&h, 2)
    );
}

/// Line 1 from 0,0 to 100,0, its end fixed; line 2 from 200,0 to 300,0,
/// its start held 100 from that end by an aligned d1.
fn tied() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "200,0", "300,0", ""]);
    h.enter_all(&["GCFIX", "100,0"]);
    h.enter_all(&["DCALIGNED", "100,0", "200,0", "150,-15", ""]);
    assert_eq!(h.session.drawing().constraints().len(), 2, "{:?}", h.log());
    holds(&h);
    h
}

#[test]
fn moving_along_one_axis_holds_the_other() {
    // #420: a MOVE by 0,50 keeps every x's bits, which left them free to
    // slide in and keep d1; moved whole, the line is refused.
    let unmoved = |h: &Harness| {
        assert_eq!(line(h, 2).start, p(200.0, 0.0));
        assert_eq!(line(h, 2).end, p(300.0, 0.0));
    };
    let mut h = tied();
    h.enter("MOVE");
    h.click_world(p(250.0, 0.0));
    h.enter_all(&["", "0,0", "0,50"]);
    assert!(refused(&h), "{:?}", h.log());
    assert!(h.log().last().unwrap().contains("d1"), "{:?}", h.log());
    unmoved(&h);

    // The same through STRETCH, its window holding all of line 2.
    let mut h = tied();
    let revision = h.session.drawing().revision();
    h.enter_all(&["STRETCH", "310,10", "190,-10", "0,0", "0,50"]);
    assert!(refused(&h), "{:?}", h.log());
    assert_eq!(h.session.drawing().revision(), revision);
    unmoved(&h);

    // And through ALIGN by one pair of points.
    let mut h = tied();
    h.enter("ALIGN");
    h.click_world(p(250.0, 0.0));
    h.enter_all(&["", "200,0", "200,50", ""]);
    assert!(refused(&h), "{:?}", h.log());
    unmoved(&h);

    // A move that keeps the distance to the fixed end is accepted.
    let mut h = tied();
    h.enter("MOVE");
    h.click_world(p(250.0, 0.0));
    h.enter_all(&["", "200,0", "100,100"]);
    holds(&h);
    assert!(!refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 2).start, p(100.0, 100.0));
}

#[test]
fn mirror_says_a_signed_distance_from_its_other_point() {
    // Reflected about its own end, a line measured d1 = 100 to the right
    // runs to the left: d1 is measured from its other end (#425), and the
    // line is the mirror image, not slid back into place (#420).
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["DCHORIZONTAL", "0,0", "100,0", "50,-15", ""]);
    h.enter_all(&["MIRROR", "ALL", "", "100,0", "100,10", "Y"]);
    assert!(!refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 1).start, p(200.0, 0.0));
    assert_eq!(line(&h, 1).end, p(100.0, 0.0));
    let constraints = h.session.drawing().constraints();
    let (_, d1) = constraints.iter().next().unwrap();
    assert_eq!(
        d1.references,
        vec![
            cad_document::Reference::new(id(1), cad_document::Part::End),
            cad_document::Reference::new(id(1), cad_document::Part::Start),
        ]
    );
    assert_eq!(
        d1.place,
        Some(p(150.0, -15.0)),
        "its dimension mirrored too"
    );
    holds(&h);
}

#[test]
fn a_midpoint_grip_moves_the_line_whole() {
    // #420: the midpoint grip translates the line, as MOVE does. Dragged
    // straight up, its ends' x keep their bits; held whole, the line does
    // not slide in x to keep d1, and the drag is refused.
    let mut h = tied();
    let dragged = LineEntity::new(p(200.0, 50.0), p(300.0, 50.0));
    let drawing = h.session.drawing();
    assert!(
        drawing
            .settled_with(&[(id(2), dragged.into())], &[id(2)])
            .is_err(),
        "the preview, held whole, shows nothing to follow"
    );
    assert!(
        drawing
            .settled_with(&[(id(2), dragged.into())], &[])
            .is_ok(),
        "held in part, it would have slid"
    );
    h.click_world(p(250.0, 0.0));
    h.click_world(p(250.0, 0.0));
    h.click_world(p(250.0, 50.0));
    assert!(refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 2).start, p(200.0, 0.0));

    // Its end grip still reshapes it: the start stays 100 from the fixed
    // end, and the line turns about it.
    h.escape();
    h.click_world(p(250.0, 0.0));
    h.click_world(p(300.0, 0.0));
    h.click_world(p(200.0, 100.0));
    holds(&h);
    assert!(!refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 2).start, p(200.0, 0.0));
    assert_eq!(line(&h, 2).end, p(200.0, 100.0));
}

/// Line 1 on layer 0 from 0,0 to 100,0; line 2 on the locked layer
/// "held" from `from` to `to`.
fn beside_a_locked_line(from: &str, to: &str) -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LA", "New", "held", "Set", "held", ""]);
    h.enter_all(&["LINE", from, to, ""]);
    h.enter_all(&["LA", "Set", "0", "Lock", "held", ""]);
    assert!(h.session.drawing().is_locked(id(2)), "{:?}", h.log());
    assert!(!h.session.drawing().is_locked(id(1)));
    h
}

#[test]
fn an_edit_that_would_move_a_locked_object_is_refused() {
    // #423: line 2 is locked; its start was dragged along by the
    // coincidence when line 1 moved.
    let mut h = beside_a_locked_line("100,0", "100,50");
    h.enter("LA");
    h.enter_all(&["Unlock", "held", ""]);
    h.enter_all(&["GCCOINCIDENT", "100,0", "100,0"]);
    h.enter_all(&["LA", "Lock", "held", ""]);
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    let revision = h.session.drawing().revision();
    h.enter("MOVE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "0,10"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The edit would move an object on a locked layer.",
        "{:?}",
        h.log()
    );
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(line(&h, 2).start, p(100.0, 0.0));
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));
    // A grip's preview settles the same way: nothing to draw following.
    let moved = LineEntity::new(p(0.0, 10.0), p(100.0, 10.0));
    assert!(
        h.session
            .drawing()
            .settled_with(&[(id(1), moved.into())], &[])
            .is_err()
    );

    // What need not move the locked line is not refused: line 1's free
    // end dragged by its grip, the joint and line 2 left where they are.
    let free = LineEntity::new(p(0.0, 20.0), p(100.0, 0.0));
    assert_eq!(
        h.session
            .drawing()
            .settled_with(&[(id(1), free.into())], &[]),
        Ok(Vec::new())
    );
    h.click_world(p(50.0, 0.0));
    h.click_world(p(0.0, 0.0));
    h.click_world(p(0.0, 20.0));
    assert_eq!(line(&h, 1), free, "{:?}", h.log());
    assert_eq!(line(&h, 2).start, p(100.0, 0.0));
    holds(&h);
    h.escape();
    h.undo();

    // Unlocked, the same move takes line 2's start along.
    h.enter_all(&["LA", "Unlock", "held", ""]);
    h.enter("MOVE");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["", "0,0", "0,10"]);
    holds(&h);
    assert!((line(&h, 2).start - p(100.0, 10.0)).length() < 1e-7);
}

#[test]
fn a_geometric_constraint_does_not_pick_a_locked_object_by_a_point() {
    // #423: the typed point 120,30 is the locked line's start; it is not
    // picked, so nothing is constrained and nothing moves.
    let mut h = beside_a_locked_line("120,30", "150,80");
    h.enter_all(&["GCCOINCIDENT", "100,0", "120,30"]);
    assert!(
        h.session.drawing().constraints().is_empty(),
        "{:?}",
        h.log()
    );
    assert_eq!(line(&h, 2).start, p(120.0, 30.0));
    assert_eq!(line(&h, 1).end, p(100.0, 0.0));
}

#[test]
fn a_tangent_circle_is_never_turned_inside_out() {
    // #424: the line moved up 40 through the circle's centre. The nearest
    // solution kept the tangency with a radius of -10; held at its radius,
    // the circle moves up with the line instead.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
        cad_document::CircleEntity::new(p(0.0, 0.0), 10.0).into(),
    ]);
    h.enter("GCTANGENT");
    h.click_world(p(30.0, -10.0));
    h.click_world(p(0.0, 10.0));
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    // The grip preview of the same move settles the same way.
    let moved = LineEntity::new(p(-50.0, 30.0), p(50.0, 30.0));
    let preview = h
        .session
        .drawing()
        .settled_with(&[(id(1), moved.into())], &[id(1)])
        .unwrap();
    assert!(
        matches!(preview.as_slice(), [(_, Entity::Circle(c))] if (c.radius - 10.0).abs() < 1e-7),
        "{preview:?}"
    );
    h.enter("MOVE");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,0", "0,40"]);
    assert_eq!(line(&h, 1).start, p(-50.0, 30.0), "{:?}", h.log());
    let Some(Entity::Circle(circle)) = h.session.drawing().entity(id(2)) else {
        panic!("a circle");
    };
    assert!((circle.radius - 10.0).abs() < 1e-7, "{circle:?}");
    assert!((circle.center.y - 40.0).abs() < 1e-7, "{circle:?}");
    holds(&h);

    // A radius the edit cannot keep positive is refused: the circle is
    // fixed at its centre, so it cannot follow the line.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
        cad_document::CircleEntity::new(p(0.0, 0.0), 10.0).into(),
    ]);
    h.enter("GCTANGENT");
    h.click_world(p(30.0, -10.0));
    h.click_world(p(0.0, 10.0));
    h.enter_all(&["GCFIX", "0,0"]);
    assert_eq!(h.session.drawing().constraints().len(), 2, "{:?}", h.log());
    let revision = h.session.drawing().revision();
    h.enter("MOVE");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,0", "0,40"]);
    assert_eq!(
        h.log().last().unwrap(),
        "The edit would leave a circle or an arc with no radius.",
        "{:?}",
        h.log()
    );
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_tangent_arc_is_never_turned_inside_out() {
    // #424: an arc's circle the same: it follows the line, its radius kept.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
        cad_document::ArcEntity::new(p(0.0, 0.0), 10.0, std::f64::consts::PI, 0.0).into(),
    ]);
    h.enter("GCTANGENT");
    h.click_world(p(30.0, -10.0));
    let below = f64::to_radians(225.0);
    h.click_world(p(10.0 * below.cos(), 10.0 * below.sin()));
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    h.enter("MOVE");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,0", "0,40"]);
    let Some(Entity::Arc(arc)) = h.session.drawing().entity(id(2)) else {
        panic!("an arc");
    };
    assert!((arc.radius - 10.0).abs() < 1e-7, "{arc:?}");
    assert!((arc.center.y - 40.0).abs() < 1e-7, "{arc:?} {:?}", h.log());
    holds(&h);
}

#[test]
fn a_real_conflict_beside_a_circle_is_still_refused_and_says_which() {
    // #424: holding the radii is a second try, not a way round a
    // constraint. With its centre fixed and its radius dimensioned, the
    // circle cannot follow the line: refused, naming a constraint.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
        cad_document::CircleEntity::new(p(0.0, 0.0), 10.0).into(),
    ]);
    h.enter("GCTANGENT");
    h.click_world(p(30.0, -10.0));
    h.click_world(p(0.0, 10.0));
    h.enter_all(&["GCFIX", "0,0"]);
    h.enter("DCRADIUS");
    h.click_world(p(10.0, 0.0));
    h.enter_all(&["20,10", "10"]);
    assert_eq!(h.session.drawing().constraints().len(), 3, "{:?}", h.log());
    let revision = h.session.drawing().revision();
    h.enter("MOVE");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,0", "0,40"]);
    let said = h.log().last().unwrap().clone();
    assert!(said.starts_with("The edit would break"), "{:?}", h.log());
    assert_eq!(h.session.drawing().revision(), revision);
}

#[test]
fn a_mirror_keeps_what_constraints_among_the_mirrored_say() {
    // #425: a pure reflection keeps every constraint whose parts all go
    // through it, once each is said again.
    use cad_document::{ArcEntity, CircleEntity, Kind, Part, Touch};

    // An arc's ends trade places: line 1's end, coincident with the arc's
    // start, is coincident with its end after the mirror.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, 0.0), p(10.0, 0.0)).into(),
        ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, std::f64::consts::FRAC_PI_2).into(),
    ]);
    h.enter_all(&["GCCOINCIDENT", "10,0", "10,0"]);
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    h.enter_all(&["MIRROR", "ALL", "", "0,100", "100,100", "Y"]);
    assert!(!refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 1).end, p(10.0, 200.0));
    let constraints = h.session.drawing().constraints();
    let (_, coincident) = constraints.iter().next().unwrap();
    assert!(
        coincident
            .references
            .contains(&cad_document::Reference::new(id(2), Part::End))
    );
    holds(&h);

    // A tangent touches the line's other side: the circle keeps its radius.
    let mut h = Harness::with_entities([
        LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
        CircleEntity::new(p(0.0, 0.0), 10.0).into(),
    ]);
    h.enter("GCTANGENT");
    h.click_world(p(30.0, -10.0));
    h.click_world(p(0.0, 10.0));
    let touch = |h: &Harness| match h.session.drawing().constraints().iter().next() {
        Some((_, c)) => c.kind.clone(),
        None => panic!("{:?}", h.log()),
    };
    let before = touch(&h);
    h.enter_all(&["MIRROR", "ALL", "", "0,100", "100,100", "Y"]);
    assert!(!refused(&h), "{:?}", h.log());
    let Some(Entity::Circle(circle)) = h.session.drawing().entity(id(2)) else {
        panic!("a circle");
    };
    assert_eq!(*circle, CircleEntity::new(p(0.0, 200.0), 10.0));
    let flipped = |kind: &Kind| match kind {
        Kind::Tangent { touch: Touch::Left } => Kind::Tangent {
            touch: Touch::Right,
        },
        Kind::Tangent {
            touch: Touch::Right,
        } => Kind::Tangent { touch: Touch::Left },
        other => other.clone(),
    };
    assert_eq!(touch(&h), flipped(&before));
    holds(&h);

    // An angle turns the other way, about a slanted mirror too.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "0,0", "50,50", ""]);
    h.enter("DCANGULAR");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(25.0, 25.0));
    h.enter_all(&["30,10", ""]);
    let angle = |h: &Harness| match h.session.drawing().constraints().iter().next() {
        Some((_, c)) => match c.kind {
            Kind::Angle { value } => value,
            ref other => panic!("{other:?}"),
        },
        None => panic!("{:?}", h.log()),
    };
    let before = angle(&h);
    h.enter_all(&["MIRROR", "ALL", "", "0,-20", "30,40", "Y"]);
    assert!(!refused(&h), "{:?}", h.log());
    assert!((angle(&h) + before).abs() < 1e-12, "{} {before}", angle(&h));
    holds(&h);
}

#[test]
fn a_constraint_across_the_mirror_is_left_to_settling() {
    // #425: only the line is mirrored, so its tangency to the circle is
    // not restated; settling keeps it by bringing the circle along, or,
    // the circle fixed, refuses the mirror.
    let tangent = |fixed: bool| {
        let mut h = Harness::with_entities([
            LineEntity::new(p(-50.0, -10.0), p(50.0, -10.0)).into(),
            cad_document::CircleEntity::new(p(0.0, 0.0), 10.0).into(),
        ]);
        h.enter("GCTANGENT");
        h.click_world(p(30.0, -10.0));
        h.click_world(p(0.0, 10.0));
        if fixed {
            h.enter_all(&["GCFIX", "0,0"]);
        }
        h
    };
    let mut h = tangent(false);
    h.enter("MIRROR");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,100", "100,100", "Y"]);
    assert!(!refused(&h), "{:?}", h.log());
    assert_eq!(line(&h, 1).start, p(-50.0, 210.0));
    let Some(Entity::Circle(circle)) = h.session.drawing().entity(id(2)) else {
        panic!("a circle");
    };
    assert!((circle.radius - 10.0).abs() < 1e-7, "{circle:?}");
    assert!((circle.center.y - 220.0).abs() < 1e-7, "{circle:?}");
    holds(&h);

    // Fixed, the circle cannot follow: the nearest settling would turn it
    // inside out to keep the tangency, so the mirror is refused (#424).
    let mut h = tangent(true);
    let revision = h.session.drawing().revision();
    h.enter("MIRROR");
    h.click_world(p(30.0, -10.0));
    h.enter_all(&["", "0,100", "100,100", "Y"]);
    assert!(
        h.log()
            .last()
            .is_some_and(|said| said.starts_with("The edit would")),
        "{:?}",
        h.log()
    );
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(line(&h, 1).start, p(-50.0, -10.0));
}
