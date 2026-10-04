//! Object snapping (DESIGN §18).
//!
//! ```text
//! cursor → pick box in pixels → spatial-index query → candidate entities
//!        → exact snap points per mode → rank → one snap point (or none)
//! ```
//!
//! The snap aperture is a display tolerance in pixels, converted through the
//! view at the moment of use; it is never a geometric tolerance (§18.3).

use cad_core::{EntityId, PixelTolerance, Point2, ScreenPoint, View};
use cad_document::{Drawing, Entity};
use cad_geometry::Primitive;

/// How close, on screen, the cursor must be to a snap point to take it.
pub const SNAP_APERTURE: PixelTolerance = PixelTolerance(10.0);
/// Beyond this many candidate entities, intersections are not searched: the
/// pair count grows quadratically, and a cursor over that much geometry is
/// not aiming at one intersection.
const MAX_INTERSECTION_CANDIDATES: usize = 24;

/// One kind of object snap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapMode {
    Endpoint,
    Midpoint,
    Center,
    Quadrant,
    Intersection,
    Nearest,
}

impl SnapMode {
    /// Every mode, in ranking order: when two snap points are equally close,
    /// the earlier mode wins.
    pub const ALL: [Self; 6] = [
        Self::Endpoint,
        Self::Intersection,
        Self::Midpoint,
        Self::Center,
        Self::Quadrant,
        Self::Nearest,
    ];

    const fn bit(self) -> u32 {
        1 << self as u32
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Endpoint => "Endpoint",
            Self::Midpoint => "Midpoint",
            Self::Center => "Center",
            Self::Quadrant => "Quadrant",
            Self::Intersection => "Intersection",
            Self::Nearest => "Nearest",
        }
    }
}

/// The running object snaps: which modes are tried while OSNAP is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapModes {
    bits: u32,
}

impl Default for SnapModes {
    /// Everything but Nearest, which would otherwise win wherever the cursor
    /// touches geometry.
    fn default() -> Self {
        let on = [
            SnapMode::Endpoint,
            SnapMode::Midpoint,
            SnapMode::Center,
            SnapMode::Quadrant,
            SnapMode::Intersection,
        ];
        Self {
            bits: on.iter().fold(0, |bits, mode| bits | mode.bit()),
        }
    }
}

impl SnapModes {
    pub const NONE: Self = Self { bits: 0 };

    pub fn only(modes: &[SnapMode]) -> Self {
        Self {
            bits: modes.iter().fold(0, |bits, mode| bits | mode.bit()),
        }
    }

    pub fn is_on(self, mode: SnapMode) -> bool {
        self.bits & mode.bit() != 0
    }

    /// Flips `mode` and returns its new state.
    pub fn flip(&mut self, mode: SnapMode) -> bool {
        self.bits ^= mode.bit();
        self.is_on(mode)
    }

    /// The modes that are on, in ranking order.
    pub fn enabled(self) -> impl Iterator<Item = SnapMode> {
        SnapMode::ALL
            .into_iter()
            .filter(move |mode| self.is_on(*mode))
    }
}

/// A point the cursor can snap to, and why.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPoint {
    pub mode: SnapMode,
    pub point: Point2,
}

/// The exact snap points of one entity for one mode. Intersection needs two
/// entities and Nearest needs the cursor, so neither is produced here.
///
/// | Mode | Line | Circle | Arc | Polyline |
/// | --- | --- | --- | --- | --- |
/// | Endpoint | both ends | — | both ends | every vertex |
/// | Midpoint | middle | — | middle of the arc | middle of every segment |
/// | Center | — | centre | centre | — |
/// | Quadrant | — | 0°, 90°, 180°, 270° | those within the sweep | — |
pub fn entity_snap_points(entity: &Entity, mode: SnapMode) -> Vec<Point2> {
    entity
        .primitives()
        .iter()
        .flat_map(|primitive| match (mode, primitive) {
            (SnapMode::Endpoint, Primitive::Segment(segment)) => vec![segment.start, segment.end],
            (SnapMode::Endpoint, Primitive::Arc(arc)) => vec![arc.start_point(), arc.end_point()],
            (SnapMode::Midpoint, Primitive::Segment(segment)) => {
                vec![segment.start + (segment.end - segment.start) * 0.5]
            }
            (SnapMode::Midpoint, Primitive::Arc(arc)) => vec![arc.mid_point()],
            (SnapMode::Center, Primitive::Circle(circle)) => vec![circle.center],
            (SnapMode::Center, Primitive::Arc(arc)) => vec![arc.center],
            (SnapMode::Quadrant, Primitive::Circle(circle)) => (0..4)
                .map(|quadrant| circle.point_at(f64::from(quadrant) * std::f64::consts::FRAC_PI_2))
                .collect(),
            (SnapMode::Quadrant, Primitive::Arc(arc)) => arc
                .quadrant_angles()
                .map(|angle| arc.point_at(angle))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// A snap point with what ranking needs: how far it is from the cursor on
/// screen, in pixels.
struct Ranked {
    snap: SnapPoint,
    pixels: f64,
}

/// The snap point for a cursor position, if any enabled mode offers one
/// within the aperture.
///
/// `excluded` are entities a transient edit is moving: they must not be
/// offered at their old, committed positions (§18.2, LOCKED).
pub fn find_snap(
    drawing: &Drawing,
    view: &View,
    cursor: ScreenPoint,
    modes: SnapModes,
    excluded: &[EntityId],
) -> Option<SnapPoint> {
    if modes == SnapModes::NONE {
        return None;
    }
    let at = view.screen_to_world(cursor);
    let pixels_to = |point: Point2| at.distance_to(point) * view.scale();
    let within = |pixels: f64| pixels <= SNAP_APERTURE.0;

    // Candidates come from the index, never from a scan of the drawing.
    let mut ids = drawing.query(&view.pick_bounds(cursor, SNAP_APERTURE));
    ids.retain(|id| !excluded.contains(id));
    ids.sort_unstable();
    let candidates: Vec<&Entity> = ids.iter().filter_map(|id| drawing.entity(*id)).collect();

    let mut found: Vec<Ranked> = Vec::new();
    for entity in &candidates {
        let on_curve = within(entity.distance_to(at) * view.scale());
        for mode in modes.enabled() {
            match mode {
                SnapMode::Intersection => {}
                SnapMode::Nearest if on_curve => {
                    let nearest = entity
                        .primitives()
                        .iter()
                        .map(|primitive| primitive.closest_point(at))
                        .min_by(|a, b| pixels_to(*a).total_cmp(&pixels_to(*b)));
                    found.extend(nearest.map(|point| Ranked {
                        snap: SnapPoint { mode, point },
                        pixels: pixels_to(point),
                    }));
                }
                SnapMode::Nearest => {}
                // Hovering a circle or arc offers its centre, as in classic
                // CAD — but ranked behind any snap point actually under the cursor.
                SnapMode::Center if on_curve => {
                    found.extend(entity_snap_points(entity, mode).into_iter().map(|point| {
                        Ranked {
                            snap: SnapPoint { mode, point },
                            pixels: pixels_to(point).min(SNAP_APERTURE.0),
                        }
                    }));
                }
                _ => found.extend(
                    entity_snap_points(entity, mode)
                        .into_iter()
                        .map(|point| Ranked {
                            snap: SnapPoint { mode, point },
                            pixels: pixels_to(point),
                        })
                        .filter(|ranked| within(ranked.pixels)),
                ),
            }
        }
    }

    if modes.is_on(SnapMode::Intersection) && candidates.len() <= MAX_INTERSECTION_CANDIDATES {
        for (index, first) in candidates.iter().enumerate() {
            for second in &candidates[index + 1..] {
                found.extend(
                    first
                        .intersections(second)
                        .into_iter()
                        .map(|point| Ranked {
                            snap: SnapPoint {
                                mode: SnapMode::Intersection,
                                point,
                            },
                            pixels: pixels_to(point),
                        })
                        .filter(|ranked| within(ranked.pixels)),
                );
            }
        }
    }

    // Nearest is the fallback tier; within a tier the closest wins, and ties
    // go to the earlier mode in `SnapMode::ALL`.
    let rank = |mode: SnapMode| {
        SnapMode::ALL
            .iter()
            .position(|candidate| *candidate == mode)
            .unwrap_or(usize::MAX)
    };
    found
        .into_iter()
        .min_by(|a, b| {
            let tier = |ranked: &Ranked| ranked.snap.mode == SnapMode::Nearest;
            tier(a)
                .cmp(&tier(b))
                .then(a.pixels.total_cmp(&b.pixels))
                .then(rank(a.snap.mode).cmp(&rank(b.snap.mode)))
        })
        .map(|ranked| ranked.snap)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use cad_document::{ArcEntity, CircleEntity, LineEntity, PolylineEntity};

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn view() -> View {
        View::new(800.0, 600.0)
    }

    /// The snap for a cursor at world `at`, in a view where one unit is one pixel.
    fn snap_at(drawing: &Drawing, at: Point2, modes: SnapModes) -> Option<SnapPoint> {
        let view = view();
        find_snap(drawing, &view, view.world_to_screen(at), modes, &[])
    }

    fn drawing(entities: Vec<Entity>) -> Drawing {
        Drawing::from_entities(entities)
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        LineEntity::new(p(x0, y0), p(x1, y1)).into()
    }

    fn hit(mode: SnapMode, x: f64, y: f64) -> Option<SnapPoint> {
        Some(SnapPoint {
            mode,
            point: p(x, y),
        })
    }

    // ── per-entity snap points ──

    #[test]
    fn snap_points_per_mode_and_entity_kind() {
        let segment = line(0.0, 0.0, 10.0, 0.0);
        assert_eq!(
            entity_snap_points(&segment, SnapMode::Endpoint),
            vec![p(0.0, 0.0), p(10.0, 0.0)]
        );
        assert_eq!(
            entity_snap_points(&segment, SnapMode::Midpoint),
            vec![p(5.0, 0.0)]
        );
        assert!(entity_snap_points(&segment, SnapMode::Center).is_empty());
        assert!(entity_snap_points(&segment, SnapMode::Quadrant).is_empty());

        let circle: Entity = CircleEntity::new(p(10.0, 10.0), 5.0).into();
        assert_eq!(
            entity_snap_points(&circle, SnapMode::Center),
            vec![p(10.0, 10.0)]
        );
        assert_eq!(entity_snap_points(&circle, SnapMode::Quadrant).len(), 4);
        assert!(entity_snap_points(&circle, SnapMode::Endpoint).is_empty());

        // A quarter arc in the first quadrant: two ends, one middle, a centre,
        // and the two axis extremes it reaches (its own ends).
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2).into();
        assert_eq!(entity_snap_points(&arc, SnapMode::Endpoint).len(), 2);
        let middle = entity_snap_points(&arc, SnapMode::Midpoint)[0];
        assert!(middle.distance_to(p(50f64.sqrt(), 50f64.sqrt())) < 1e-9);
        assert_eq!(
            entity_snap_points(&arc, SnapMode::Center),
            vec![p(0.0, 0.0)]
        );
        assert_eq!(entity_snap_points(&arc, SnapMode::Quadrant).len(), 2);
        let half: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.1, PI - 0.1).into();
        assert_eq!(
            entity_snap_points(&half, SnapMode::Quadrant).len(),
            1,
            "only 90° lies inside"
        );

        let polyline: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], true).into();
        assert_eq!(
            entity_snap_points(&polyline, SnapMode::Endpoint).len(),
            6,
            "two per segment, shared vertices twice"
        );
        assert_eq!(entity_snap_points(&polyline, SnapMode::Midpoint).len(), 3);
    }

    // ── the engine ──

    #[test]
    fn the_snapped_point_is_exact_not_the_cursor() {
        let d = drawing(vec![line(0.0, 0.0, 100.0, 0.0)]);
        assert_eq!(
            snap_at(&d, p(97.0, 4.0), SnapModes::default()),
            hit(SnapMode::Endpoint, 100.0, 0.0)
        );
        assert_eq!(
            snap_at(&d, p(52.0, -3.0), SnapModes::default()),
            hit(SnapMode::Midpoint, 50.0, 0.0)
        );
    }

    #[test]
    fn nothing_within_the_aperture_means_no_snap() {
        let d = drawing(vec![line(0.0, 0.0, 100.0, 0.0)]);
        assert_eq!(
            snap_at(&d, p(80.0, 0.0), SnapModes::default()),
            None,
            "on the line, far from any snap point"
        );
        assert_eq!(
            snap_at(&d, p(100.0, 40.0), SnapModes::default()),
            None,
            "off the line"
        );
        assert_eq!(
            snap_at(&d, p(100.0, 0.0), SnapModes::NONE),
            None,
            "nothing enabled"
        );
    }

    #[test]
    fn the_closest_snap_point_wins() {
        let d = drawing(vec![line(0.0, 0.0, 10.0, 0.0)]);
        // Endpoint (10,0) is 4 away; midpoint (5,0) is 1 away.
        assert_eq!(
            snap_at(&d, p(6.0, 0.0), SnapModes::default()),
            hit(SnapMode::Midpoint, 5.0, 0.0)
        );
        assert_eq!(
            snap_at(&d, p(9.0, 0.0), SnapModes::default()),
            hit(SnapMode::Endpoint, 10.0, 0.0)
        );
    }

    #[test]
    fn ties_go_to_the_earlier_mode() {
        // Two lines sharing an endpoint, which is also their intersection.
        let d = drawing(vec![line(0.0, 0.0, 50.0, 0.0), line(50.0, 0.0, 50.0, 50.0)]);
        assert_eq!(
            snap_at(&d, p(49.0, 1.0), SnapModes::default()),
            hit(SnapMode::Endpoint, 50.0, 0.0)
        );
    }

    #[test]
    fn only_enabled_modes_are_offered() {
        let d = drawing(vec![line(0.0, 0.0, 100.0, 0.0)]);
        let midpoint_only = SnapModes::only(&[SnapMode::Midpoint]);
        assert_eq!(snap_at(&d, p(99.0, 0.0), midpoint_only), None);
        assert_eq!(
            snap_at(&d, p(51.0, 0.0), midpoint_only),
            hit(SnapMode::Midpoint, 50.0, 0.0)
        );
    }

    #[test]
    fn hovering_a_circle_offers_its_centre_but_real_snap_points_come_first() {
        let d = drawing(vec![Entity::from(CircleEntity::new(p(0.0, 0.0), 100.0))]);
        // On the curve at 45°, far from any quadrant: the centre.
        let on_curve = p(
            100.0 * std::f64::consts::FRAC_1_SQRT_2,
            100.0 * std::f64::consts::FRAC_1_SQRT_2,
        );
        assert_eq!(
            snap_at(&d, on_curve, SnapModes::default()),
            hit(SnapMode::Center, 0.0, 0.0)
        );
        // Near the east quadrant point: the quadrant, which is under the cursor.
        assert_eq!(
            snap_at(&d, p(99.0, 3.0), SnapModes::default()),
            hit(SnapMode::Quadrant, 100.0, 0.0)
        );
        // Near the centre itself.
        assert_eq!(
            snap_at(&d, p(2.0, -2.0), SnapModes::default()),
            hit(SnapMode::Center, 0.0, 0.0)
        );
        // Inside, away from both: nothing.
        assert_eq!(snap_at(&d, p(40.0, 40.0), SnapModes::default()), None);
    }

    #[test]
    fn intersections_of_two_entities_are_snap_points() {
        let d = drawing(vec![
            line(0.0, 0.0, 100.0, 100.0),
            line(0.0, 100.0, 100.0, 0.0),
        ]);
        assert_eq!(
            snap_at(&d, p(53.0, 49.0), SnapModes::default()),
            hit(SnapMode::Intersection, 50.0, 50.0)
        );

        // A line through a circle: two intersections; the one under the cursor is taken.
        let d = drawing(vec![
            line(-200.0, 30.0, 200.0, 30.0),
            Entity::from(CircleEntity::new(p(0.0, 0.0), 50.0)),
        ]);
        assert_eq!(
            snap_at(&d, p(41.0, 31.0), SnapModes::default()),
            hit(SnapMode::Intersection, 40.0, 30.0)
        );
        assert_eq!(
            snap_at(&d, p(-41.0, 31.0), SnapModes::default()),
            hit(SnapMode::Intersection, -40.0, 30.0)
        );
    }

    #[test]
    fn a_polyline_intersects_other_entities_segment_by_segment() {
        let square: Entity = PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            true,
        )
        .into();
        let d = drawing(vec![square, line(50.0, -50.0, 50.0, 150.0)]);
        // The vertical crosses the bottom edge at its midpoint: both snaps are
        // at the same place, and Intersection ranks before Midpoint.
        assert_eq!(
            snap_at(&d, p(51.0, 1.0), SnapModes::default()),
            hit(SnapMode::Intersection, 50.0, 0.0)
        );
        assert_eq!(
            snap_at(&d, p(49.0, 101.0), SnapModes::default()),
            hit(SnapMode::Intersection, 50.0, 100.0)
        );
    }

    /// #114: arc segments of a polyline snap like arcs.
    #[test]
    fn the_arc_segments_of_a_polyline_offer_arc_snaps() {
        // A slot 200 long and 80 high with semicircular ends of radius 40.
        let slot: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(200.0, 0.0), p(200.0, 80.0), p(0.0, 80.0)],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        )
        .into();
        assert_eq!(
            entity_snap_points(&slot, SnapMode::Center),
            vec![p(200.0, 40.0), p(0.0, 40.0)]
        );
        let d = drawing(vec![slot]);

        // The outermost point of the right end is both the arc's midpoint
        // and a quadrant point; either name is right, the point is what counts.
        let found = snap_at(&d, p(238.0, 41.0), SnapModes::default()).unwrap();
        assert!(matches!(
            found.mode,
            SnapMode::Midpoint | SnapMode::Quadrant
        ));
        assert!(found.point.distance_to(p(240.0, 40.0)) < 1e-9);

        // Hovering the arc away from its snap points offers its centre.
        let on_the_arc = p(200.0 + 40.0 * 0.6, 40.0 + 40.0 * 0.8);
        assert_eq!(
            snap_at(&d, on_the_arc, SnapModes::default()),
            hit(SnapMode::Center, 200.0, 40.0)
        );
        // The chord between the two vertices is not there to snap to.
        assert_eq!(
            snap_at(&d, p(200.0, 20.0), SnapModes::only(&[SnapMode::Nearest])),
            None
        );
    }

    #[test]
    fn nearest_is_the_fallback_and_lands_exactly_on_the_curve() {
        let d = drawing(vec![line(0.0, 0.0, 100.0, 0.0)]);
        let all = SnapModes::only(&SnapMode::ALL);
        // Far from the ends and the middle: nearest, projected onto the line.
        assert_eq!(
            snap_at(&d, p(80.0, 4.0), all),
            hit(SnapMode::Nearest, 80.0, 0.0)
        );
        // Where a real snap point is in reach, it wins although Nearest is closer.
        assert_eq!(
            snap_at(&d, p(96.0, 1.0), all),
            hit(SnapMode::Endpoint, 100.0, 0.0)
        );
    }

    /// DESIGN §18.2 (LOCKED): entities being transiently edited are not snap targets.
    #[test]
    fn excluded_entities_are_never_offered() {
        let d = drawing(vec![
            line(0.0, 0.0, 100.0, 0.0),
            line(100.0, 0.0, 100.0, 80.0),
        ]);
        let ids: Vec<EntityId> = {
            let mut ids: Vec<EntityId> = d.entities().map(|(id, _)| id).collect();
            ids.sort_unstable();
            ids
        };
        let view = view();
        let cursor = view.world_to_screen(p(1.0, 1.0));

        let all = find_snap(&d, &view, cursor, SnapModes::default(), &[]);
        assert_eq!(all, hit(SnapMode::Endpoint, 0.0, 0.0));
        // With the first line excluded, its endpoint at the origin is gone…
        assert_eq!(
            find_snap(&d, &view, cursor, SnapModes::default(), &ids[..1]),
            None
        );
        // …and so is its intersection with the other line, while the other
        // line's own endpoint stays.
        let corner = view.world_to_screen(p(99.0, 1.0));
        assert_eq!(
            find_snap(&d, &view, corner, SnapModes::default(), &ids[..1]),
            hit(SnapMode::Endpoint, 100.0, 0.0)
        );
    }

    #[test]
    fn the_aperture_is_constant_in_pixels_at_any_zoom() {
        let d = drawing(vec![line(0.0, 0.0, 100.0, 0.0)]);
        for factor in [1.0, 20.0, 0.05] {
            let mut view = view();
            view.zoom_about(view.world_to_screen(p(100.0, 0.0)), factor);
            let end = view.world_to_screen(p(100.0, 0.0));
            let near = ScreenPoint::new(end.x + 8.0, end.y);
            let far = ScreenPoint::new(end.x + 13.0, end.y);
            let endpoint_only = SnapModes::only(&[SnapMode::Endpoint]);
            assert!(
                find_snap(&d, &view, near, endpoint_only, &[]).is_some(),
                "8px at ×{factor}"
            );
            assert!(
                find_snap(&d, &view, far, endpoint_only, &[]).is_none(),
                "13px at ×{factor}"
            );
        }
    }

    #[test]
    fn running_snaps_can_be_flipped() {
        let mut modes = SnapModes::default();
        assert!(!modes.is_on(SnapMode::Nearest));
        assert!(modes.flip(SnapMode::Nearest));
        assert!(!modes.flip(SnapMode::Endpoint));
        let enabled: Vec<SnapMode> = modes.enabled().collect();
        assert_eq!(
            enabled,
            vec![
                SnapMode::Intersection,
                SnapMode::Midpoint,
                SnapMode::Center,
                SnapMode::Quadrant,
                SnapMode::Nearest
            ]
        );
    }

    #[test]
    fn crowded_areas_skip_the_intersection_search() {
        // Thirty lines through one point: more candidates than the cap.
        let lines = (0..30).map(|i| {
            let angle = f64::from(i) * PI / 30.0;
            line(
                -50.0 * angle.cos(),
                -50.0 * angle.sin(),
                50.0 * angle.cos(),
                50.0 * angle.sin(),
            )
        });
        let d = drawing(lines.collect());
        // Every line's midpoint is the origin, so a snap is still found — just not by intersecting 435 pairs.
        assert_eq!(
            snap_at(&d, p(1.0, 1.0), SnapModes::default()),
            hit(SnapMode::Midpoint, 0.0, 0.0)
        );
    }
}
