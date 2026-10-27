//! A building's elevations (#337, ADR-0040): each façade as it is seen
//! square on from one side, worked out from the plan's outer walls, its
//! windows and doors, and the brief's heights and roof. In metres on the
//! façade: `u` along it from its left end as seen, `z` up from the ground.
//!
//! - The walls are the footprint's side seen, from the ground to the top
//!   of the wall — of the parapet over a flat roof, of the ridge under a
//!   gable seen end on.
//! - Each window and door of that side is its frame, from its sill or the
//!   ground to its head, with the frame's inside 5 cm in; a window has its
//!   sill below it.
//! - A pitched roof's ridge runs along the building's longer side. Seen
//!   from that side, a gable roof is the band from its eaves to its ridge
//!   and a hip roof a trapezoid; seen end on, a gable roof is the verge's
//!   two slopes and a hip roof the triangle of its hipped end.

use cad_core::Point2;

use crate::{Rect, Resolved, Roof, Spec};

/// The sides a building is seen from: the front faces the street, along
/// the terrain's x axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
    Left,
    Right,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::Front, Self::Back, Self::Left, Self::Right];

    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Back => "Back",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

/// A frame's width: how far its inside is from its outside.
pub const FRAME: f64 = 0.05;
/// A pitched roof's thickness, measured upright.
pub const ROOF_THICKNESS: f64 = 0.2;
/// How far the ground line runs past the building at each end.
pub const GROUND_PAST: f64 = 1.0;

/// A façade, as closed outlines and lines on the façade's plane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Elevation {
    /// The walls' outline.
    pub walls: Vec<Vec<Point2>>,
    /// Each opening's frame outside and in, and a window's sill.
    pub openings: Vec<Vec<Point2>>,
    /// The roof's outlines.
    pub roof: Vec<Vec<Point2>>,
    /// Lines across the façade: the slab's edges, a fascia.
    pub lines: Vec<(Point2, Point2)>,
    pub ground: (Point2, Point2),
    /// The façade's length, and its height to its highest point.
    pub width: f64,
    pub height: f64,
}

fn p(u: f64, z: f64) -> Point2 {
    Point2::new(u, z)
}

fn rectangle(u0: f64, z0: f64, u1: f64, z1: f64) -> Vec<Point2> {
    vec![p(u0, z0), p(u1, z0), p(u1, z1), p(u0, z1)]
}

/// Where a point of the walls' centreline is along the façade of `side`,
/// from its left end as seen; `None` when the point is not on that side's
/// wall.
fn along(side: Side, outline: Rect, footprint: Rect, point: Point2, tolerance: f64) -> Option<f64> {
    let near = |a: f64, b: f64| (a - b).abs() <= tolerance;
    match side {
        Side::Front => near(point.y, outline.y).then_some(point.x - footprint.x),
        Side::Back => near(point.y, outline.top()).then(|| footprint.right() - point.x),
        Side::Left => near(point.x, outline.x).then(|| footprint.top() - point.y),
        Side::Right => near(point.x, outline.right()).then_some(point.y - footprint.y),
    }
}

/// The façade of the plan seen from `side`: its layout's and openings'
/// as drawn (`Resolved`), the brief's heights and roof.
pub fn elevation(spec: &Spec, plan: &Resolved, side: Side) -> Elevation {
    let layout = &plan.layout;
    let building = spec.building;
    let footprint = layout.footprint;
    let outline = layout.outline;
    let tolerance = 1e-6 * (footprint.w + footprint.h).max(1.0);
    let (width, depth) = match side {
        Side::Front | Side::Back => (footprint.w, footprint.h),
        Side::Left | Side::Right => (footprint.h, footprint.w),
    };
    let wall = spec.storey_height;
    let mut out = Elevation {
        width,
        ground: (p(-GROUND_PAST, 0.0), p(width + GROUND_PAST, 0.0)),
        ..Elevation::default()
    };

    // The roof: whether this side sees the eaves — the ridge runs along
    // it — and the slopes' heights.
    let ridge_along_x = footprint.w >= footprint.h;
    let sees_eaves = matches!(side, Side::Front | Side::Back) == ridge_along_x;
    let rise = building.roof_pitch.to_radians().tan();
    let e = building.eaves;
    match building.roof {
        Roof::Flat => {
            let top = wall + building.slab + building.parapet;
            out.walls.push(rectangle(0.0, 0.0, width, top));
            // The slab's edge, a band across the façade.
            for z in [wall, wall + building.slab] {
                out.lines.push((p(0.0, z), p(width, z)));
            }
            out.height = top;
        }
        Roof::Gable | Roof::Hip => {
            // The span across the ridge, and the heights of the eaves'
            // underside, of the ridge's, and of the ridge's top.
            let span = if sees_eaves { depth } else { width };
            let under_eaves = wall - e * rise;
            let ridge = wall + span / 2.0 * rise;
            let top = ridge + ROOF_THICKNESS;
            out.height = top;
            if sees_eaves {
                out.walls.push(rectangle(0.0, 0.0, width, wall));
                let (left, right) = (-e, width + e);
                let roof = if building.roof == Roof::Gable {
                    rectangle(left, under_eaves, right, top)
                } else {
                    // The ridge between the hips, a span's half from each end.
                    let half = (span / 2.0).min(width / 2.0);
                    vec![
                        p(left, under_eaves),
                        p(right, under_eaves),
                        p(width - half, top),
                        p(half, top),
                    ]
                };
                out.roof.push(roof);
                out.lines.push((
                    p(left, under_eaves + ROOF_THICKNESS),
                    p(right, under_eaves + ROOF_THICKNESS),
                ));
            } else if building.roof == Roof::Gable {
                // The gable wall up to the roof, and the verge's slopes.
                out.walls.push(vec![
                    p(0.0, 0.0),
                    p(width, 0.0),
                    p(width, wall),
                    p(width / 2.0, ridge),
                    p(0.0, wall),
                ]);
                out.roof.push(vec![
                    p(-e, under_eaves),
                    p(width / 2.0, ridge),
                    p(width + e, under_eaves),
                    p(width + e, under_eaves + ROOF_THICKNESS),
                    p(width / 2.0, top),
                    p(-e, under_eaves + ROOF_THICKNESS),
                ]);
            } else {
                // A hipped end: the wall to its top, and the hip's
                // triangle over it.
                out.walls.push(rectangle(0.0, 0.0, width, wall));
                out.roof.push(vec![
                    p(-e, under_eaves),
                    p(width + e, under_eaves),
                    p(width / 2.0, top),
                ]);
                out.lines.push((
                    p(-e, under_eaves + ROOF_THICKNESS),
                    p(width + e, under_eaves + ROOF_THICKNESS),
                ));
            }
        }
    }

    // The openings on this side.
    let span_of = |from: Point2, to: Point2| {
        let a = along(side, outline, footprint, from, tolerance)?;
        let b = along(side, outline, footprint, to, tolerance)?;
        Some((a.min(b), a.max(b)))
    };
    for window in &plan.windows {
        if let Some((u0, u1)) = span_of(window.from, window.to) {
            let (sill, head) = (building.sill, building.window_head);
            out.openings.push(rectangle(u0, sill, u1, head));
            out.openings.push(rectangle(
                u0 + FRAME,
                sill + FRAME,
                u1 - FRAME,
                head - FRAME,
            ));
            // The sill, a frame's width proud of the opening each way.
            out.openings
                .push(rectangle(u0 - FRAME, sill - FRAME, u1 + FRAME, sill));
        }
    }
    for door in plan.doors.iter().filter(|door| door.rooms.1.is_none()) {
        if let Some((u0, u1)) = span_of(door.from, door.to) {
            let head = building.door_head;
            out.openings.push(rectangle(u0, 0.0, u1, head));
            out.openings
                .push(rectangle(u0 + FRAME, 0.0, u1 - FRAME, head - FRAME));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Building, Room, Setbacks, Terrain, solve};

    fn house(roof: Roof) -> (Spec, Resolved) {
        let spec = Spec {
            terrain: Terrain {
                width: 14.0,
                depth: 20.0,
            },
            setbacks: Setbacks {
                front: 3.0,
                ..Setbacks::default()
            },
            rooms: vec![
                Room {
                    entrance: true,
                    ..Room::new("living", 24.0)
                },
                Room::new("kitchen", 10.0),
                Room::new("bedroom", 12.0),
            ],
            adjacent: vec![["living".into(), "kitchen".into()]],
            exterior_wall: 0.25,
            interior_wall: 0.15,
            door: 0.9,
            max_circulation: 0.15,
            windows: Some(1.2),
            storey_height: 2.8,
            building: Building {
                roof,
                ..Building::default()
            },
            seed: 0,
        };
        let layout = solve(&spec).unwrap();
        let plan = Resolved::of(&spec, &layout);
        (spec, plan)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn bounds(points: &[Point2]) -> (f64, f64, f64, f64) {
        points.iter().fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |(a, b, c, d), q| (a.min(q.x), b.min(q.y), c.max(q.x), d.max(q.y)),
        )
    }

    #[test]
    fn each_facade_is_the_buildings_side_with_its_openings_at_their_heights() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        let fp = layout.footprint;
        let all_windows = plan.windows.clone();
        let mut seen = 0;
        for side in Side::ALL {
            let e = elevation(&spec, &plan, side);
            let width = if matches!(side, Side::Front | Side::Back) {
                fp.w
            } else {
                fp.h
            };
            assert!(close(e.width, width));
            // The wall: the side's length, up to the parapet.
            let (u0, z0, u1, z1) = bounds(&e.walls[0]);
            assert!(close(u0, 0.0) && close(u1, width) && close(z0, 0.0));
            assert!(close(z1, 2.8 + 0.2 + 0.5));
            // Each opening's frame lies within the wall, a window between
            // its sill and head, a door from the ground.
            for frame in e.openings.chunks(1) {
                let (a, b, c, d) = bounds(&frame[0]);
                assert!(a >= -FRAME - 1e-9 && c <= width + FRAME + 1e-9, "{side:?}");
                assert!(b >= 0.9 - FRAME - 1e-9 || close(b, 0.0), "{side:?}");
                assert!(d <= 2.1 + 1e-9);
            }
            // A window is three outlines: its frame, the frame's inside,
            // its sill; a door two.
            let doors_here = if side == Side::Front { 1 } else { 0 };
            let windows_here = (e.openings.len() - 2 * doors_here) / 3;
            assert_eq!(
                e.openings.len(),
                3 * windows_here + 2 * doors_here,
                "{side:?}"
            );
            seen += windows_here;
        }
        // Every window of the plan is on one façade.
        assert_eq!(seen, all_windows.len());
    }

    #[test]
    fn a_window_is_where_the_plan_has_it_and_as_wide() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        let window = plan
            .windows
            .clone()
            .into_iter()
            .find(|w| (w.from.y - layout.outline.y).abs() < 1e-6)
            .expect("a window in the front wall");
        let e = elevation(&spec, &plan, Side::Front);
        let wanted = (
            window.from.x.min(window.to.x) - layout.footprint.x,
            window.from.x.max(window.to.x) - layout.footprint.x,
        );
        assert!(e.openings.iter().any(|o| {
            let (a, b, c, d) = bounds(o);
            close(a, wanted.0) && close(c, wanted.1) && close(b, 0.9) && close(d, 2.1)
        }));
    }

    #[test]
    fn a_gable_shows_its_eaves_along_the_ridge_and_its_slopes_end_on() {
        let (spec, plan) = house(Roof::Gable);
        let layout = &plan.layout;
        let fp = layout.footprint;
        let rise = 30_f64.to_radians().tan();
        // The ridge runs along the longer side.
        let (long_side, end_side) = if fp.w >= fp.h {
            (Side::Front, Side::Left)
        } else {
            (Side::Left, Side::Front)
        };
        let span = fp.w.min(fp.h);
        let ridge = 2.8 + span / 2.0 * rise;
        let eaves = elevation(&spec, &plan, long_side);
        let (a, b, c, d) = bounds(&eaves.roof[0]);
        assert!(close(a, -0.5) && close(c, eaves.width + 0.5));
        assert!(close(b, 2.8 - 0.5 * rise) && close(d, ridge + ROOF_THICKNESS));
        let end = elevation(&spec, &plan, end_side);
        // The gable wall's apex under the ridge, in the middle.
        assert!(
            end.walls[0]
                .iter()
                .any(|q| close(q.x, end.width / 2.0) && close(q.y, ridge))
        );
        assert!(close(end.height, ridge + ROOF_THICKNESS));
    }

    #[test]
    fn a_hip_roof_is_a_trapezoid_along_and_a_triangle_end_on() {
        let (spec, plan) = house(Roof::Hip);
        let layout = &plan.layout;
        let fp = layout.footprint;
        let (long_side, end_side) = if fp.w >= fp.h {
            (Side::Front, Side::Left)
        } else {
            (Side::Left, Side::Front)
        };
        let along = elevation(&spec, &plan, long_side);
        assert_eq!(along.roof[0].len(), 4);
        let span = fp.w.min(fp.h);
        // The ridge stops a half-span from each end.
        assert!(along.roof[0].iter().any(|q| close(q.x, span / 2.0)));
        let end = elevation(&spec, &plan, end_side);
        assert_eq!(end.roof[0].len(), 3);
        assert_eq!(end.walls[0].len(), 4, "a hipped end's wall is square");
    }
}
