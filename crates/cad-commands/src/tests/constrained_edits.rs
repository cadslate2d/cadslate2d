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

/// Line 1 from 0,0 to 100,0, line 2 from 100,0 to 100,50, line 1's end
/// coincident with line 2's start.
fn cornered() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "100,0", "100,50", ""]);
    h.enter_all(&["GCCOINCIDENT", "100,0", "100,0"]);
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    h
}

fn untouched(h: &Harness) {
    assert_eq!(line(h, 2).start, p(100.0, 0.0), "{:?}", h.log());
    assert_eq!(line(h, 2).end, p(100.0, 50.0));
}

#[test]
fn a_broken_line_hands_its_far_end_constraint_to_the_far_piece() {
    // #421: the far end is the second piece's now; line 2 stays put.
    let mut h = cornered();
    h.enter("BREAK");
    h.click_world(p(30.0, 0.0));
    h.enter("60,0");
    untouched(&h);
    assert_eq!(line(&h, 1).end, p(30.0, 0.0));
    let far = h
        .session
        .drawing()
        .entities()
        .find_map(|(id, entity)| {
            entity
                .as_line()
                .is_some_and(|l| l.end == p(100.0, 0.0))
                .then_some(id)
        })
        .expect("the far piece");
    let constraints = h.session.drawing().constraints();
    let (_, coincident) = constraints.iter().next().expect("kept");
    assert!(coincident.refers_to(far) && !coincident.refers_to(id(1)));
    holds(&h);
    // One undo puts the line and the constraint back.
    h.undo();
    assert_eq!(line(&h, 1).end, p(100.0, 0.0));
    let constraints = h.session.drawing().constraints();
    assert!(constraints.iter().next().unwrap().1.refers_to(id(1)));
}

#[test]
fn a_trimmed_end_takes_its_constraint_away() {
    // #421: the end coincident with line 2 is cut away, so the
    // coincidence goes with it instead of dragging line 2 to the cut.
    let mut h = cornered();
    h.enter_all(&["LINE", "70,-10", "70,10", ""]);
    h.enter("TRIM");
    h.click_world(p(70.0, 5.0));
    h.enter("");
    h.click_world(p(85.0, 0.0));
    untouched(&h);
    assert_eq!(line(&h, 1).end, p(70.0, 0.0));
    assert!(h.session.drawing().constraints().is_empty());
    holds(&h);
}

#[test]
fn a_joined_line_keeps_its_length_and_its_outer_ends_constraints() {
    // #421: d1 measured line 1's ends; its end is a joint now, inside the
    // joined line, so d1 goes and the line keeps all 150.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "100,0", "150,0", ""]);
    h.enter_all(&["DCHORIZONTAL", "0,0", "100,0", "50,-15", ""]);
    h.enter_all(&["JOIN", "ALL", ""]);
    assert_eq!(
        h.log().last().unwrap(),
        "2 objects joined into 1 line.",
        "{:?}",
        h.log()
    );
    assert_eq!(line(&h, 1).start, p(0.0, 0.0));
    assert_eq!(line(&h, 1).end, p(150.0, 0.0));
    assert!(h.session.drawing().constraints().is_empty());

    // An outer end's constraint is the joined line's now: line 3 stays.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "100,0", ""]);
    h.enter_all(&["LINE", "100,0", "150,0", ""]);
    h.enter_all(&["LINE", "150,0", "150,30", ""]);
    h.enter_all(&["GCCOINCIDENT", "150,0", "150,0"]);
    h.enter("JOIN");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(125.0, 0.0));
    h.enter("");
    assert_eq!(line(&h, 1).end, p(150.0, 0.0), "{:?}", h.log());
    assert_eq!(line(&h, 3).start, p(150.0, 0.0));
    let constraints = h.session.drawing().constraints();
    let (_, coincident) = constraints.iter().next().expect("carried");
    assert!(coincident.refers_to(id(1)) && coincident.refers_to(id(3)));
    holds(&h);
}

#[test]
fn an_equal_length_goes_with_a_cut_and_an_equal_radius_stays() {
    // #421: an equal length is said of the whole line, which a cut or a
    // join changes; kept, it dragged the other line to the piece's length.
    let equal = |extra: Option<(f64, f64)>| {
        let mut h = Harness::new();
        h.enter_all(&["LINE", "0,0", "100,0", ""]);
        h.enter_all(&["LINE", "0,50", "100,50", ""]);
        if let Some((from, to)) = extra {
            h.enter_all(&["LINE", &format!("{from},0"), &format!("{to},0"), ""]);
        }
        h.enter("GCEQUAL");
        h.click_world(p(50.0, 0.0));
        h.click_world(p(50.0, 50.0));
        assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
        h
    };
    let kept = |h: &Harness| {
        assert_eq!(line(h, 2).start, p(0.0, 50.0), "{:?}", h.log());
        assert_eq!(line(h, 2).end, p(100.0, 50.0));
        assert!(h.session.drawing().constraints().is_empty());
    };
    let mut h = equal(None);
    h.enter("BREAK");
    h.click_world(p(30.0, 0.0));
    h.enter("60,0");
    assert_eq!(line(&h, 1).end, p(30.0, 0.0));
    kept(&h);

    let mut h = equal(Some((100.0, 150.0)));
    h.enter("JOIN");
    h.click_world(p(50.0, 0.0));
    h.click_world(p(125.0, 0.0));
    h.enter("");
    assert_eq!(line(&h, 1).end, p(150.0, 0.0));
    kept(&h);

    // An arc's edge is its circle, which a cut keeps: its radius stays
    // equal to the circle's.
    let mut h = Harness::with_entities([
        cad_document::ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, std::f64::consts::PI).into(),
        cad_document::CircleEntity::new(p(200.0, 0.0), 10.0).into(),
    ]);
    h.enter("GCEQUAL");
    h.click_world(p(0.0, 10.0));
    h.click_world(p(210.0, 0.0));
    assert_eq!(h.session.drawing().constraints().len(), 1, "{:?}", h.log());
    let at = |degrees: f64| {
        let a = f64::to_radians(degrees);
        p(10.0 * a.cos(), 10.0 * a.sin())
    };
    h.enter("BREAK");
    h.click_world(at(60.0));
    let second = at(120.0);
    h.enter(&format!("{},{}", second.x, second.y));
    assert_eq!(h.session.drawing().len(), 3, "{:?}", h.log());
    let constraints = h.session.drawing().constraints();
    let (_, kept) = constraints.iter().next().expect("the equal radius kept");
    assert!(kept.refers_to(id(1)), "{:?}", h.log());
    holds(&h);
}

#[test]
fn a_join_that_closes_a_loop_drops_the_constraints_on_its_joints() {
    // #421: each joint's two ends are one vertex of the closed polyline;
    // carried there, a coincidence would name that vertex twice.
    let mut h = Harness::new();
    h.enter_all(&["LINE", "0,0", "10,0", ""]);
    h.enter_all(&["LINE", "10,0", "10,10", ""]);
    h.enter_all(&["LINE", "10,10", "0,0", ""]);
    for corner in ["10,0", "10,10", "0,0"] {
        h.enter_all(&["GCCOINCIDENT", corner, corner]);
    }
    assert_eq!(h.session.drawing().constraints().len(), 3, "{:?}", h.log());
    h.enter_all(&["JOIN", "ALL", ""]);
    let Some(Entity::Polyline(joined)) = h.session.drawing().entity(id(1)) else {
        panic!("{:?}", h.log());
    };
    assert!(joined.closed, "{:?}", h.log());
    assert!(h.session.drawing().constraints().is_empty());
    holds(&h);
}

/// A polyline 0,0 – 100,0 – 100,50, its last vertex coincident with the
/// start of line 2 (100,50 – 150,80) and its second segment plumb.
fn bent() -> Harness {
    let mut h = Harness::with_entities([
        cad_document::PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)], false)
            .into(),
        LineEntity::new(p(100.0, 50.0), p(150.0, 80.0)).into(),
    ]);
    h.enter_all(&["GCCOINCIDENT", "100,50", "100,50"]);
    h.enter("GCVERTICAL");
    h.click_world(p(100.0, 25.0));
    assert_eq!(h.session.drawing().constraints().len(), 2, "{:?}", h.log());
    h
}

fn vertices(h: &Harness) -> Vec<Point2> {
    match h.session.drawing().entity(id(1)) {
        Some(Entity::Polyline(polyline)) => polyline.vertices.clone(),
        other => panic!("{other:?}"),
    }
}

/// The parts of the polyline its constraints name, in row order.
fn parts_on_the_polyline(h: &Harness) -> Vec<cad_document::Part> {
    h.session
        .drawing()
        .constraints()
        .iter()
        .flat_map(|(_, c)| c.references.clone())
        .filter(|r| r.entity == id(1))
        .map(|r| r.part)
        .collect()
}

#[test]
fn a_vertex_inserted_by_pedit_leaves_the_others_constraints_where_they_were() {
    // #422: inserted after vertex 0, the old vertex 2 is vertex 3 and the
    // old segment 1 is segment 2; line 2 stays where it was.
    use cad_document::Part;
    let mut h = bent();
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["E", "I", "50,-10", "E", ""]);
    assert_eq!(
        vertices(&h),
        vec![p(0.0, 0.0), p(50.0, -10.0), p(100.0, 0.0), p(100.0, 50.0)],
        "{:?}",
        h.log()
    );
    assert_eq!(line(&h, 2).start, p(100.0, 50.0));
    assert_eq!(
        parts_on_the_polyline(&h),
        vec![Part::Vertex(3), Part::Segment(2)]
    );
    holds(&h);

    // A vertex moved keeps its number: what is on it follows.
    let mut h = bent();
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["E", "N", "N", "M", "100,70", "E", ""]);
    assert_eq!(vertices(&h)[2], p(100.0, 70.0), "{:?}", h.log());
    assert!((line(&h, 2).start - p(100.0, 70.0)).length() < 1e-7);
    holds(&h);
}

#[test]
fn a_pedit_join_carries_the_polylines_constraints_to_where_they_are() {
    // #422: a line joined at the polyline's start puts a vertex before
    // the others; what was on vertex 2 is on the vertex at 100,50 still.
    let mut h = bent();
    h.enter_all(&["LINE", "-50,0", "0,0", ""]);
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter("J");
    h.click_world(p(-25.0, 0.0));
    h.enter_all(&["", ""]);
    let joined = vertices(&h);
    assert_eq!(joined.len(), 4, "{:?}", h.log());
    assert_eq!(line(&h, 2).start, p(100.0, 50.0));
    let at = joined.iter().position(|v| *v == p(100.0, 50.0)).unwrap() as u32;
    assert!(
        parts_on_the_polyline(&h).contains(&cad_document::Part::Vertex(at)),
        "{:?}",
        parts_on_the_polyline(&h)
    );
    holds(&h);
}

#[test]
fn a_line_made_a_polyline_by_pedit_keeps_its_constraints_on_its_vertices_and_segment() {
    // #422: the line's ends are vertices 0 and 1, its edge segment 0;
    // the conversion carried nothing before, and Join then had nothing.
    use cad_document::Part;
    let constrained = || {
        let mut h = Harness::new();
        h.enter_all(&["LINE", "0,0", "100,0", ""]);
        h.enter_all(&["LINE", "100,0", "100,50", ""]);
        h.enter_all(&["GCCOINCIDENT", "100,0", "100,0"]);
        h.enter("GCHORIZONTAL");
        h.click_world(p(50.0, 0.0));
        assert_eq!(h.session.drawing().constraints().len(), 2, "{:?}", h.log());
        h
    };
    let mut h = constrained();
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["Y", ""]);
    assert_eq!(
        vertices(&h),
        vec![p(0.0, 0.0), p(100.0, 0.0)],
        "{:?}",
        h.log()
    );
    assert_eq!(
        parts_on_the_polyline(&h),
        vec![Part::Vertex(1), Part::Segment(0)]
    );
    holds(&h);

    // Made a polyline and joined: the corner is inside it, so the
    // coincidence goes; the level is still segment 0's.
    let mut h = constrained();
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["Y", "J"]);
    h.click_world(p(100.0, 25.0));
    h.enter_all(&["", ""]);
    assert_eq!(
        vertices(&h),
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
        "{:?}",
        h.log()
    );
    assert_eq!(parts_on_the_polyline(&h), vec![Part::Segment(0)]);
    holds(&h);
}

#[test]
fn a_vertex_inserted_into_a_constrained_segment_takes_its_constraint_and_undo_restores_it() {
    // #422: a vertex inserted into the plumb segment makes two segments,
    // neither between its ends, so the plumb goes; PEDIT's Undo puts back
    // the vertex and the rows as they were.
    use cad_document::Part;
    let mut h = bent();
    h.enter("PEDIT");
    h.click_world(p(50.0, 0.0));
    h.enter_all(&["E", "N", "I", "120,25"]);
    assert_eq!(vertices(&h).len(), 4, "{:?}", h.log());
    assert_eq!(parts_on_the_polyline(&h), vec![Part::Vertex(3)]);
    assert_eq!(line(&h, 2).start, p(100.0, 50.0));
    holds(&h);
    h.enter_all(&["E", "U"]);
    assert_eq!(
        vertices(&h),
        vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
        "{:?}",
        h.log()
    );
    assert_eq!(
        parts_on_the_polyline(&h),
        vec![Part::Vertex(2), Part::Segment(1)]
    );
    holds(&h);
}
