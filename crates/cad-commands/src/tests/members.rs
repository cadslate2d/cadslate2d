//! MEMBERS (#329): centrelines made into members with a width — the
//! maintainer's truss, by command; one undo; centrelines kept or not; what
//! is not a line left out; the refusals.

use cad_core::Point2;
use cad_document::{Entity, LineEntity};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A Warren roof truss: span 1200, rise 200, chords and six diagonals.
fn truss() -> Vec<Entity> {
    let rise = |x: f64| {
        if x <= 600.0 {
            x / 3.0
        } else {
            (1200.0 - x) / 3.0
        }
    };
    let mut lines = vec![
        LineEntity::new(p(0.0, 0.0), p(1200.0, 0.0)).into(),
        LineEntity::new(p(0.0, 0.0), p(600.0, 200.0)).into(),
        LineEntity::new(p(600.0, 200.0), p(1200.0, 0.0)).into(),
    ];
    for k in 1..7 {
        let (x0, x1) = (150.0 * f64::from(k), 150.0 * f64::from(k + 1));
        let (a, b) = if k % 2 == 0 {
            (p(x0, 0.0), p(x1, rise(x1)))
        } else {
            (p(x0, rise(x0)), p(x1, 0.0))
        };
        lines.push(LineEntity::new(a, b).into());
    }
    lines
}

fn outlines(h: &Harness) -> Vec<cad_document::PolylineEntity> {
    h.entities()
        .into_iter()
        .filter_map(|e| match e {
            Entity::Polyline(p) if p.closed => Some(p),
            _ => None,
        })
        .collect()
}

fn lines(h: &Harness) -> usize {
    h.entities()
        .iter()
        .filter(|e| matches!(e, Entity::Line(_)))
        .count()
}

#[test]
fn a_truss_is_made_of_chords_10_wide_and_webs_5_wide_as_one_undo() {
    let mut h = Harness::with_entities(truss());
    let ids: Vec<cad_core::EntityId> = h.session.drawing().entities().map(|(id, _)| id).collect();
    // The chords selected first, then the command.
    h.session.select_only(&ids[..3]);
    h.enter("MEMBERS");
    assert_eq!(h.prompt(), "Specify member width:");
    h.enter("10");
    assert_eq!(
        h.prompt(),
        "Select members of another width, or press Enter when they are all chosen or [Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.session.select_only(&ids[3..]);
    h.enter("");
    assert_eq!(h.prompt(), "Specify member width <10>:");
    h.enter("5");
    h.enter("");
    assert_eq!(h.prompt(), "Keep the centrelines? [Yes/No] <No>:");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.log().last().unwrap(), "9 members made from 9 lines.");
    assert_eq!(lines(&h), 0, "the centrelines went");
    let made = outlines(&h);
    assert_eq!(made.len(), 9);
    // The chords' outlines reach half a chord's width past the centrelines;
    // the webs lie between the chords' faces.
    let lowest = made
        .iter()
        .flat_map(|o| o.vertices.iter().map(|v| v.y))
        .fold(f64::MAX, f64::min);
    assert!((lowest + 5.0).abs() < 1e-6, "{lowest}");
    h.undo();
    assert!(outlines(&h).is_empty());
    assert_eq!(lines(&h), 9, "one undo brings the centrelines back");
}

#[test]
fn centrelines_may_be_kept_and_what_is_not_a_line_is_left_out() {
    let mut h = Harness::with_entities([
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into(),
        cad_document::CircleEntity::new(p(50.0, 50.0), 5.0).into(),
    ]);
    h.enter_all(&["MEMBERS", "ALL", ""]);
    assert_eq!(
        h.log().last().unwrap(),
        "Only lines are made members: 1 other object left out."
    );
    h.enter_all(&["4", "", "Y"]);
    assert_eq!(outlines(&h).len(), 1);
    assert_eq!(lines(&h), 1, "kept");
    // A width that is not a size is asked again; nothing chosen is said.
    h.enter_all(&["MEMBERS", "ALL", ""]);
    h.enter("0");
    assert_eq!(
        h.log().last().unwrap(),
        "A member's width must be greater than zero."
    );
    h.session.key(crate::KeyAction::Escape);
    h.enter_all(&["MEMBERS", ""]);
    assert_eq!(h.log().last().unwrap(), "No lines chosen.");
}
