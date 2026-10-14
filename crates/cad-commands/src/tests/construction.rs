//! XLINE and RAY (#206; ADR-0031): construction lines drawn, snapped
//! to, cut, and left out of the extents.

use cad_core::{Point2, Vec2};
use cad_document::{Entity, LineEntity, RayEntity, XLineEntity};

use crate::testing::Harness;
use crate::{Dirty, SnapMode, SnapModes, Toggle};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn close(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-9
}

fn xline(h: &Harness, index: usize) -> XLineEntity {
    match &h.entities()[index] {
        Entity::XLine(line) => *line,
        other => panic!("not an xline: {other:?}"),
    }
}

#[test]
fn xline_through_points_horizontal_vertical_at_an_angle_and_bisecting() {
    let mut h = Harness::new();
    h.enter("XLINE");
    assert_eq!(
        h.prompt(),
        "Specify a point or [Hor/Ver/Ang/Bisect/Offset]:"
    );
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify through point:");
    let dirty = h.enter("10,10");
    assert!(dirty.contains(Dirty::SCENE | Dirty::OVERLAY));
    h.enter_all(&["0,10", ""]);
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.entities().len(), 2, "one per through point");
    let first = xline(&h, 0);
    assert_eq!(first.base, p(0.0, 0.0));
    assert!((first.direction - Vec2::new(1.0, 1.0) * (1.0 / 2.0_f64.sqrt())).length() < 1e-12);
    h.undo();
    assert!(h.entities().is_empty(), "the command is one undo");

    h.enter_all(&["XL", "H", "0,5", "0,15", ""]);
    assert_eq!(xline(&h, 0).direction, Vec2::new(1.0, 0.0));
    assert_eq!(xline(&h, 1).base, p(0.0, 15.0));
    h.enter_all(&["XL", "V", "7,0", ""]);
    assert_eq!(xline(&h, 2).direction, Vec2::new(0.0, 1.0));
    h.enter_all(&["XL", "A", "30", "0,0", ""]);
    let angled = xline(&h, 3);
    assert!((angled.direction.y.atan2(angled.direction.x).to_degrees() - 30.0).abs() < 1e-9);
    // Bisecting a right angle at the origin: along 45°.
    h.enter_all(&["XL", "B", "0,0", "10,0", "0,10", ""]);
    let bisector = xline(&h, 4);
    assert!(close(bisector.base, p(0.0, 0.0)));
    assert!((bisector.direction.x - bisector.direction.y).abs() < 1e-12);
    h.enter_all(&["XL", "B", "0,0", "0,0"]);
    assert_eq!(last(&h), "The start point must differ from the vertex.");
    h.escape();
}

#[test]
fn xline_offset_makes_parallels_at_a_distance_or_through_a_point() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into()]);
    h.enter_all(&["ZOOM", "E", "XL", "O"]);
    assert_eq!(
        h.prompt(),
        "Specify offset distance or [Through] <Through>:"
    );
    h.enter("-2");
    assert_eq!(last(&h), "The offset distance must be a positive number.");
    h.enter("20");
    assert_eq!(h.prompt(), "Select a line object:");
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.prompt(), "Specify side to offset:");
    h.enter("50,-3");
    assert_eq!(h.prompt(), "Select a line object:", "it repeats");
    h.enter("");
    let parallel = xline(&h, 1);
    assert!((parallel.base.y + 20.0).abs() < 1e-9);
    assert_eq!(parallel.direction, Vec2::new(1.0, 0.0));
    h.enter_all(&["XL", "O"]);
    assert_eq!(
        h.prompt(),
        "Specify offset distance or [Through] <Through>:"
    );
    h.enter("T");
    h.click_world(p(50.0, 0.0));
    assert_eq!(h.prompt(), "Specify through point:");
    h.enter_all(&["10,7", ""]);
    assert!((xline(&h, 2).base.y - 7.0).abs() < 1e-9);
}

#[test]
fn ray_from_a_start_through_each_point() {
    let mut h = Harness::new();
    h.enter("RAY");
    assert_eq!(h.prompt(), "Specify start point:");
    h.enter("5,5");
    assert_eq!(h.prompt(), "Specify through point:");
    h.enter("5,5");
    assert_eq!(last(&h), "The through point must differ from the start.");
    h.enter_all(&["15,5", "5,15", ""]);
    assert_eq!(
        h.entities(),
        vec![
            RayEntity::through(p(5.0, 5.0), p(15.0, 5.0))
                .unwrap()
                .into(),
            RayEntity::through(p(5.0, 5.0), p(5.0, 15.0))
                .unwrap()
                .into(),
        ]
    );
}

#[test]
fn construction_lines_snap_at_their_base_nearest_perpendicular_and_crossings() {
    let mut h = Harness::new();
    h.enter_all(&["XL", "H", "0,0", ""]);
    h.enter_all(&["RAY", "50,-50", "50,50", ""]);
    h.enter_all(&["L", "-30,-30", "-30,30", ""]);
    h.enter_all(&["ZOOM", "C", "0,0", "200"]);
    // Where the xline crosses the ray, and the line, far from any base.
    h.enter("L");
    h.move_to_world(p(50.5, 0.5));
    let state = h.session.ui_state();
    assert_eq!(state.snap, Some("Intersection"));
    let (x, y) = state.cursor.unwrap();
    assert!(close(p(x, y), p(50.0, 0.0)), "{x}, {y}");
    h.move_to_world(p(-29.5, 0.5));
    assert_eq!(h.session.ui_state().cursor, Some((-30.0, 0.0)));
    // The ray's start as an endpoint; the xline's base as its midpoint.
    h.move_to_world(p(50.5, -49.5));
    assert_eq!(h.session.ui_state().snap, Some("Endpoint"));
    h.escape();
    // Midpoint alone: the others switched off.
    h.enter_all(&["OS", "E", "C", "Q", "I", "INS", "NOD", ""]);
    assert_eq!(
        h.session.snap_modes(),
        SnapModes::only(&[SnapMode::Midpoint])
    );
    h.enter("L");
    h.move_to_world(p(0.5, 0.5));
    assert_eq!(h.session.ui_state().snap, Some("Midpoint"));
    assert_eq!(h.session.ui_state().cursor, Some((0.0, 0.0)));
    h.escape();
    // Nearest, anywhere along it.
    h.enter_all(&["OS", "M", "N", ""]);
    h.enter("L");
    h.move_to_world(p(77.0, 0.8));
    assert_eq!(h.session.ui_state().cursor, Some((77.0, 0.0)));
    h.escape();
    // Perpendicular from a point.
    h.enter_all(&["OS", "N", "P", ""]);
    h.enter_all(&["L", "20,40"]);
    h.move_to_world(p(25.0, 0.5));
    assert_eq!(h.session.ui_state().snap, Some("Perpendicular"));
    let (x, y) = h.session.ui_state().cursor.unwrap();
    assert!(close(p(x, y), p(20.0, 0.0)), "{x}, {y}");
    h.escape();
    let _ = Toggle::Osnap;
}

#[test]
fn trim_turns_an_xline_into_rays_and_a_ray_into_a_line() {
    let mut h = Harness::new();
    h.enter_all(&["XL", "H", "0,0", ""]);
    h.enter_all(&["L", "10,-10", "10,10", "", "L", "30,-10", "30,10", ""]);
    h.enter_all(&["ZOOM", "C", "20,0", "80"]);
    // Both lines as edges; the xline picked between them.
    h.enter("TRIM");
    h.click_world(p(10.0, 5.0));
    h.click_world(p(30.0, 5.0));
    h.enter("");
    h.click_world(p(20.0, 0.0));
    h.enter("");
    let rays: Vec<RayEntity> = h
        .entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Ray(ray) => Some(ray),
            _ => None,
        })
        .collect();
    assert_eq!(rays.len(), 2, "{:?}", h.entities());
    assert!(
        rays.iter()
            .any(|ray| close(ray.start, p(10.0, 0.0)) && ray.direction.x < 0.0)
    );
    assert!(
        rays.iter()
            .any(|ray| close(ray.start, p(30.0, 0.0)) && ray.direction.x > 0.0)
    );
    // The ray going right, trimmed beyond the line at x = 30: a line.
    h.enter_all(&["L", "50,-10", "50,10", ""]);
    h.enter("TRIM");
    h.click_world(p(50.0, 5.0));
    h.enter("");
    h.click_world(p(70.0, 0.0));
    h.enter("");
    assert!(
        h.entities()
            .iter()
            .any(|entity| *entity == LineEntity::new(p(30.0, 0.0), p(50.0, 0.0)).into()),
        "{:?}",
        h.entities()
    );
}

#[test]
fn zoom_extents_ignores_construction_lines_and_the_overlay_draws_them_in_the_view() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 0.0), p(100.0, 50.0)).into()]);
    h.enter_all(&["XL", "0,1000", "1,1000", ""]);
    h.enter_all(&["RAY", "5000,5000", "6000,5000", ""]);
    h.enter_all(&["ZOOM", "E"]);
    let view = h.session.view();
    assert!(close(view.center(), p(50.0, 25.0)), "{:?}", view.center());
    // Panning onto the xline draws it there, and leaves the drawing alone.
    let revision = h.session.drawing().revision();
    h.enter_all(&["ZOOM", "C", "123456,1000", "50"]);
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(h.session.drawing().shown().count(), 3);
    assert_eq!(
        h.session
            .drawing()
            .query_shown(&h.session.view().visible_bounds())
            .len(),
        1,
        "the xline is found where it crosses the view"
    );
}
