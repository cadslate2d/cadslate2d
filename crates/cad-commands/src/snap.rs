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
use cad_document::{Drawing, Entity, LayoutId, Measure, Tables};
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
    /// The foot of the perpendicular from the point being drawn from.
    Perpendicular,
    /// The point of tangency of a line from the point being drawn from.
    Tangent,
    /// The insertion point of a text (or, later, a block).
    Insertion,
}

impl SnapMode {
    /// Every mode, in ranking order: when two snap points are equally close,
    /// the earlier mode wins.
    pub const ALL: [Self; 9] = [
        Self::Endpoint,
        Self::Intersection,
        Self::Midpoint,
        Self::Insertion,
        Self::Perpendicular,
        Self::Tangent,
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
            Self::Perpendicular => "Perpendicular",
            Self::Tangent => "Tangent",
            Self::Insertion => "Insertion",
        }
    }
}

/// The running object snaps: which modes are tried while OSNAP is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapModes {
    bits: u32,
}

impl Default for SnapModes {
    /// The modes that name a point on an object. Nearest, Perpendicular, and
    /// Tangent are off: with them on, merely touching an object snaps.
    fn default() -> Self {
        let on = [
            SnapMode::Endpoint,
            SnapMode::Midpoint,
            SnapMode::Center,
            SnapMode::Quadrant,
            SnapMode::Intersection,
            SnapMode::Insertion,
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
/// entities, Nearest needs the cursor, and Perpendicular and Tangent need
/// the point being drawn from, so none of those is produced here.
///
/// | Mode | Line | Circle | Arc | Polyline |
/// | --- | --- | --- | --- | --- |
/// | Endpoint | both ends | — | both ends | every vertex |
/// | Midpoint | middle | — | middle of the arc | middle of every segment |
/// | Center | — | centre | centre | — |
/// | Quadrant | — | 0°, 90°, 180°, 270° | those within the sweep | — |
///
/// A polyline's arc segments offer what an arc offers. Perpendicular applies
/// to every kind (the foot must land on the object); Tangent to circles and
/// arcs, from a point outside them.
pub fn entity_snap_points(entity: &Entity, mode: SnapMode, tables: Tables<'_>) -> Vec<Point2> {
    // A text offers its insertion point and nothing else: not the ends of
    // its glyphs' strokes (ADR-0016). It has no primitives, so the rest
    // of this finds nothing on it either.
    let insertion = match entity {
        Entity::Text(text) => Some(text.insertion),
        Entity::MText(text) => Some(text.insertion),
        Entity::AttributeDefinition(definition) => Some(definition.text.insertion),
        _ => None,
    };
    if let Some(insertion) = insertion {
        return match mode {
            SnapMode::Insertion => vec![insertion],
            _ => Vec::new(),
        };
    }
    // A block reference offers its insertion point, and its definition's
    // geometry offers what it would on its own.
    if let (Entity::BlockRef(reference), SnapMode::Insertion) = (entity, mode) {
        return vec![reference.insertion];
    }
    // A dimension offers the points it measures as endpoints, its vertex
    // or centre as a centre, and its text's insertion point: not its
    // lines, which are not geometry of the drawing (ADR-0020).
    if let Entity::Dimension(dimension) = entity {
        return match mode {
            SnapMode::Endpoint => dimension.measure.points(),
            SnapMode::Center => match dimension.measure {
                Measure::Angular { vertex, .. } => vec![vertex],
                Measure::Radius { center, .. } | Measure::Diameter { center, .. } => vec![center],
                Measure::Linear { .. } | Measure::Aligned { .. } => Vec::new(),
            },
            SnapMode::Insertion => vec![
                dimension
                    .layout(tables.dim_style(dimension.style))
                    .text
                    .insertion,
            ],
            _ => Vec::new(),
        };
    }
    // A viewport offers its frame's corners, their midpoints, and its
    // centre (#192): a window on the paper, not geometry of the drawing.
    if let Entity::Viewport(viewport) = entity {
        return match mode {
            SnapMode::Endpoint => viewport.corners().to_vec(),
            SnapMode::Midpoint => viewport
                .edges()
                .iter()
                .map(|edge| edge.start + (edge.end - edge.start) * 0.5)
                .collect(),
            SnapMode::Center => vec![viewport.center],
            _ => Vec::new(),
        };
    }
    // An ellipse offers its centre, its axis ends, and an arc's ends and
    // middle: not the ends of its chords (#181).
    if let Entity::Ellipse(entity) = entity {
        let ellipse = entity.ellipse();
        return match mode {
            SnapMode::Center => vec![ellipse.center],
            SnapMode::Quadrant => ellipse
                .quadrant_parameters()
                .into_iter()
                .map(|t| ellipse.point_at(t))
                .collect(),
            SnapMode::Endpoint if !ellipse.is_full() => {
                vec![ellipse.start_point(), ellipse.end_point()]
            }
            SnapMode::Midpoint if !ellipse.is_full() => {
                vec![ellipse.point_at(ellipse.start + ellipse.sweep / 2.0)]
            }
            _ => Vec::new(),
        };
    }
    // A spline offers its ends and its middle: not the ends of its chords.
    if let Entity::Spline(entity) = entity {
        let spline = entity.spline();
        let (start, end) = spline.domain();
        return match mode {
            SnapMode::Endpoint => vec![spline.start_point(), spline.end_point()],
            SnapMode::Midpoint => vec![spline.point_at((start + end) / 2.0)],
            _ => Vec::new(),
        };
    }
    entity
        .primitives(tables)
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

/// How a candidate came to be offered, from strongest claim to weakest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    /// The snap point itself is under the cursor.
    UnderCursor,
    /// A construction from the base point (Perpendicular, Tangent), offered
    /// because the cursor is on the object it lands on.
    Construction,
    /// The centre of a circle or arc whose curve the cursor is on.
    HoveredCenter,
    /// The point of the object nearest the cursor: the fallback.
    Nearest,
}

/// A snap point with what ranking needs: why it is offered, and how far it
/// is from the cursor on screen, in pixels.
struct Ranked {
    snap: SnapPoint,
    tier: Tier,
    pixels: f64,
}

impl Ranked {
    /// A snap point under the cursor.
    fn at(mode: SnapMode, point: Point2, pixels: f64) -> Self {
        Self {
            snap: SnapPoint { mode, point },
            tier: Tier::UnderCursor,
            pixels,
        }
    }

    /// A snap point offered because the cursor is on its object, wherever
    /// on the object the point itself is. Under the cursor it is as good as
    /// any other snap point; elsewhere it ranks in `tier`.
    fn hovered(mode: SnapMode, point: Point2, pixels: f64, tier: Tier) -> Self {
        Self {
            tier: if pixels <= SNAP_APERTURE.0 {
                Tier::UnderCursor
            } else {
                tier
            },
            ..Self::at(mode, point, pixels)
        }
    }
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
    find_snap_from(
        drawing,
        LayoutId::MODEL,
        view,
        cursor,
        modes,
        excluded,
        None,
    )
}

/// [`find_snap`], with the point the command is drawing from. Perpendicular
/// and Tangent are constructions *from* that point, so they are offered only
/// when there is one.
pub fn find_snap_from(
    drawing: &Drawing,
    space: LayoutId,
    view: &View,
    cursor: ScreenPoint,
    modes: SnapModes,
    excluded: &[EntityId],
    base: Option<Point2>,
) -> Option<SnapPoint> {
    if modes == SnapModes::NONE {
        return None;
    }
    let at = view.screen_to_world(cursor);
    let pixels_to = |point: Point2| at.distance_to(point) * view.scale();
    let within = |pixels: f64| pixels <= SNAP_APERTURE.0;

    // Candidates come from the index, never from a scan of the drawing.
    // What is on a layer that is off is not there to snap to.
    let mut ids = drawing.query_shown_in(space, &view.pick_bounds(cursor, SNAP_APERTURE));
    ids.retain(|id| !excluded.contains(id));
    ids.sort_unstable();
    let candidates: Vec<&Entity> = ids.iter().filter_map(|id| drawing.entity(*id)).collect();

    let mut found: Vec<Ranked> = Vec::new();
    for entity in &candidates {
        let on_curve = within(entity.distance_to(at, drawing.tables()) * view.scale());
        for mode in modes.enabled() {
            match mode {
                SnapMode::Intersection => {}
                SnapMode::Nearest if on_curve => {
                    // An ellipse's nearest point is on its curve, not a chord.
                    let nearest = match entity {
                        Entity::Ellipse(ellipse) => Some(ellipse.ellipse().closest_point(at)),
                        Entity::Spline(spline) => Some(spline.spline().closest_point(at)),
                        _ => entity
                            .primitives(drawing.tables())
                            .iter()
                            .map(|primitive| primitive.closest_point(at))
                            .min_by(|a, b| pixels_to(*a).total_cmp(&pixels_to(*b))),
                    };
                    found.extend(nearest.map(|point| Ranked {
                        tier: Tier::Nearest,
                        ..Ranked::at(mode, point, pixels_to(point))
                    }));
                }
                SnapMode::Nearest => {}
                // Hovering a circle or arc offers its centre, as in classic
                // CAD — but ranked behind any snap point actually under the cursor.
                SnapMode::Center if on_curve => {
                    found.extend(
                        entity_snap_points(entity, mode, drawing.tables())
                            .into_iter()
                            .map(|point| {
                                Ranked::hovered(mode, point, pixels_to(point), Tier::HoveredCenter)
                            }),
                    );
                }
                // Constructions from the base point: offered while the cursor
                // is on the piece of the object they land on, or on the point itself.
                SnapMode::Perpendicular | SnapMode::Tangent => {
                    let Some(base) = base else { continue };
                    for primitive in entity.primitives(drawing.tables()) {
                        let touching = within(primitive.distance_to_point(at) * view.scale());
                        let points = match mode {
                            SnapMode::Perpendicular => primitive.perpendicular_feet(base),
                            _ => primitive.tangent_points(base),
                        };
                        for point in points.as_slice() {
                            let pixels = pixels_to(*point);
                            if touching {
                                found.push(Ranked::hovered(
                                    mode,
                                    *point,
                                    pixels,
                                    Tier::Construction,
                                ));
                            } else if within(pixels) {
                                found.push(Ranked::at(mode, *point, pixels));
                            }
                        }
                    }
                }
                _ => found.extend(
                    entity_snap_points(entity, mode, drawing.tables())
                        .into_iter()
                        .map(|point| Ranked::at(mode, point, pixels_to(point)))
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
                        .intersections(second, drawing.tables())
                        .into_iter()
                        .map(|point| Ranked::at(SnapMode::Intersection, point, pixels_to(point)))
                        .filter(|ranked| within(ranked.pixels)),
                );
            }
        }
    }

    // The strongest claim wins; within a tier the closest to the cursor, and
    // ties go to the earlier mode in `SnapMode::ALL`.
    let rank = |mode: SnapMode| {
        SnapMode::ALL
            .iter()
            .position(|candidate| *candidate == mode)
            .unwrap_or(usize::MAX)
    };
    found
        .into_iter()
        .min_by(|a, b| {
            a.tier
                .cmp(&b.tier)
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
            entity_snap_points(&segment, SnapMode::Endpoint, cad_document::Tables::none()),
            vec![p(0.0, 0.0), p(10.0, 0.0)]
        );
        assert_eq!(
            entity_snap_points(&segment, SnapMode::Midpoint, cad_document::Tables::none()),
            vec![p(5.0, 0.0)]
        );
        assert!(
            entity_snap_points(&segment, SnapMode::Center, cad_document::Tables::none()).is_empty()
        );
        assert!(
            entity_snap_points(&segment, SnapMode::Quadrant, cad_document::Tables::none())
                .is_empty()
        );

        let circle: Entity = CircleEntity::new(p(10.0, 10.0), 5.0).into();
        assert_eq!(
            entity_snap_points(&circle, SnapMode::Center, cad_document::Tables::none()),
            vec![p(10.0, 10.0)]
        );
        assert_eq!(
            entity_snap_points(&circle, SnapMode::Quadrant, cad_document::Tables::none()).len(),
            4
        );
        assert!(
            entity_snap_points(&circle, SnapMode::Endpoint, cad_document::Tables::none())
                .is_empty()
        );

        // A quarter arc in the first quadrant: two ends, one middle, a centre,
        // and the two axis extremes it reaches (its own ends).
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2).into();
        assert_eq!(
            entity_snap_points(&arc, SnapMode::Endpoint, cad_document::Tables::none()).len(),
            2
        );
        let middle = entity_snap_points(&arc, SnapMode::Midpoint, cad_document::Tables::none())[0];
        assert!(middle.distance_to(p(50f64.sqrt(), 50f64.sqrt())) < 1e-9);
        assert_eq!(
            entity_snap_points(&arc, SnapMode::Center, cad_document::Tables::none()),
            vec![p(0.0, 0.0)]
        );
        assert_eq!(
            entity_snap_points(&arc, SnapMode::Quadrant, cad_document::Tables::none()).len(),
            2
        );
        let half: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.1, PI - 0.1).into();
        assert_eq!(
            entity_snap_points(&half, SnapMode::Quadrant, cad_document::Tables::none()).len(),
            1,
            "only 90° lies inside"
        );

        let polyline: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], true).into();
        assert_eq!(
            entity_snap_points(&polyline, SnapMode::Endpoint, cad_document::Tables::none()).len(),
            6,
            "two per segment, shared vertices twice"
        );
        assert_eq!(
            entity_snap_points(&polyline, SnapMode::Midpoint, cad_document::Tables::none()).len(),
            3
        );
    }

    // ── the engine ──

    #[test]
    fn a_dimension_offers_its_points_its_centre_and_its_text_and_nothing_on_its_lines() {
        use cad_document::{DimensionEntity, Measure, Tables};
        let radius = Entity::from(DimensionEntity::new(
            Measure::Radius {
                center: p(0.0, 0.0),
                radius: 50.0,
            },
            p(20.0, 0.0),
        ));
        let of = |entity: &Entity, mode| entity_snap_points(entity, mode, Tables::none());
        assert_eq!(of(&radius, SnapMode::Endpoint), vec![p(0.0, 0.0)]);
        assert_eq!(of(&radius, SnapMode::Center), vec![p(0.0, 0.0)]);
        assert!(of(&radius, SnapMode::Midpoint).is_empty());
        assert!(of(&radius, SnapMode::Quadrant).is_empty());
        let insertion = of(&radius, SnapMode::Insertion);
        assert_eq!(insertion.len(), 1);
        assert!(insertion[0].y > 0.0, "the text sits above the leader");

        let linear = Entity::from(DimensionEntity::new(
            Measure::Linear {
                a: p(0.0, 0.0),
                b: p(100.0, 0.0),
                rotation: 0.0,
            },
            p(0.0, 20.0),
        ));
        assert_eq!(
            of(&linear, SnapMode::Endpoint),
            vec![p(0.0, 0.0), p(100.0, 0.0)],
            "what it measures, not the ends of its lines"
        );
        assert!(of(&linear, SnapMode::Center).is_empty());
        assert!(of(&linear, SnapMode::Midpoint).is_empty());
        // Through the drawing: the cursor on the dimension line snaps to
        // nothing of it; near a measured point, to that point.
        let drawing = drawing(vec![linear]);
        assert!(
            snap_at(
                &drawing,
                p(50.0, 20.0),
                SnapModes::only(&[SnapMode::Midpoint, SnapMode::Nearest])
            )
            .is_none()
        );
        let snapped = snap_at(
            &drawing,
            p(99.0, 1.0),
            SnapModes::only(&[SnapMode::Endpoint]),
        )
        .unwrap();
        assert_eq!(snapped.point, p(100.0, 0.0));
    }

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
            entity_snap_points(&slot, SnapMode::Center, cad_document::Tables::none()),
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

    // ── perpendicular and tangent (#117) ──

    fn snap_from(
        drawing: &Drawing,
        base: Point2,
        at: Point2,
        modes: &[SnapMode],
    ) -> Option<SnapPoint> {
        let view = view();
        let cursor = view.world_to_screen(at);
        find_snap_from(
            drawing,
            LayoutId::MODEL,
            &view,
            cursor,
            SnapModes::only(modes),
            &[],
            Some(base),
        )
    }

    const PERPENDICULAR: &[SnapMode] = &[SnapMode::Perpendicular];
    const TANGENT: &[SnapMode] = &[SnapMode::Tangent];

    #[test]
    fn perpendicular_lands_on_the_foot_wherever_the_line_is_hovered() {
        let d = drawing(vec![line(0.0, 0.0, 200.0, 0.0)]);
        let base = p(60.0, 90.0);
        // Hovering the line far from the foot still offers the foot.
        assert_eq!(
            snap_from(&d, base, p(150.0, 2.0), PERPENDICULAR),
            hit(SnapMode::Perpendicular, 60.0, 0.0)
        );
        assert_eq!(
            snap_from(&d, base, p(61.0, -3.0), PERPENDICULAR),
            hit(SnapMode::Perpendicular, 60.0, 0.0)
        );
        // Off the line: nothing.
        assert_eq!(snap_from(&d, base, p(150.0, 40.0), PERPENDICULAR), None);
    }

    #[test]
    fn perpendicular_needs_a_base_point_and_a_foot_on_the_object() {
        let d = drawing(vec![line(0.0, 0.0, 200.0, 0.0)]);
        // No base point: nothing to construct from.
        assert_eq!(
            snap_at(&d, p(150.0, 2.0), SnapModes::only(PERPENDICULAR)),
            None
        );
        // The perpendicular from here misses the segment.
        assert_eq!(
            snap_from(&d, p(260.0, 90.0), p(150.0, 2.0), PERPENDICULAR),
            None
        );
        // A base point on the line itself has no perpendicular to drop.
        assert_eq!(
            snap_from(&d, p(60.0, 0.0), p(150.0, 2.0), PERPENDICULAR),
            None
        );
    }

    #[test]
    fn perpendicular_to_a_circle_takes_the_foot_on_the_side_that_is_hovered() {
        let d = drawing(vec![Entity::from(CircleEntity::new(p(0.0, 0.0), 50.0))]);
        let base = p(200.0, 0.0);
        // Near side and far side of the line through the centre.
        let near = snap_from(&d, base, p(48.0, 14.0), PERPENDICULAR).unwrap();
        assert!(near.point.distance_to(p(50.0, 0.0)) < 1e-9, "{near:?}");
        let far = snap_from(&d, base, p(-48.0, 14.0), PERPENDICULAR).unwrap();
        assert!(far.point.distance_to(p(-50.0, 0.0)) < 1e-9, "{far:?}");
        // From the centre every direction is perpendicular: nothing is offered.
        assert_eq!(
            snap_from(&d, p(0.0, 0.0), p(48.0, 14.0), PERPENDICULAR),
            None
        );
    }

    #[test]
    fn perpendicular_works_on_arcs_and_polyline_segments() {
        // The upper half circle of radius 50.
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
        let d = drawing(vec![arc]);
        let found = snap_from(&d, p(0.0, 200.0), p(30.0, 40.0), PERPENDICULAR).unwrap();
        assert!(found.point.distance_to(p(0.0, 50.0)) < 1e-9);
        // From below, the near foot would be on the missing lower half.
        let found = snap_from(&d, p(0.0, -200.0), p(30.0, 40.0), PERPENDICULAR).unwrap();
        assert!(
            found.point.distance_to(p(0.0, 50.0)) < 1e-9,
            "only the far foot exists"
        );

        let corner: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], false).into();
        let d = drawing(vec![corner]);
        let base = p(40.0, 60.0);
        // Hovering the bottom segment offers the foot on it, not on the other one.
        assert_eq!(
            snap_from(&d, base, p(80.0, 1.0), PERPENDICULAR),
            hit(SnapMode::Perpendicular, 40.0, 0.0)
        );
        assert_eq!(
            snap_from(&d, base, p(99.0, 30.0), PERPENDICULAR),
            hit(SnapMode::Perpendicular, 100.0, 60.0)
        );
    }

    #[test]
    fn tangent_lands_on_the_point_of_tangency_on_the_side_that_is_hovered() {
        // Radius 30 seen from 50 away: a 3-4-5 triangle, tangent points at (18, ±24).
        let d = drawing(vec![Entity::from(CircleEntity::new(p(0.0, 0.0), 30.0))]);
        let base = p(50.0, 0.0);
        let upper = snap_from(&d, base, p(0.0, 30.0), TANGENT).unwrap();
        assert_eq!(upper.mode, SnapMode::Tangent);
        assert!(upper.point.distance_to(p(18.0, 24.0)) < 1e-9, "{upper:?}");
        let lower = snap_from(&d, base, p(0.0, -30.0), TANGENT).unwrap();
        assert!(lower.point.distance_to(p(18.0, -24.0)) < 1e-9, "{lower:?}");
    }

    #[test]
    fn there_is_no_tangent_from_inside_a_circle_or_to_a_line() {
        let d = drawing(vec![
            Entity::from(CircleEntity::new(p(0.0, 0.0), 30.0)),
            line(-100.0, 80.0, 100.0, 80.0),
        ]);
        assert_eq!(
            snap_from(&d, p(5.0, 5.0), p(0.0, 30.0), TANGENT),
            None,
            "base inside the circle"
        );
        assert_eq!(
            snap_from(&d, p(30.0, 0.0), p(0.0, 30.0), TANGENT),
            None,
            "base on the circle"
        );
        assert_eq!(
            snap_from(&d, p(0.0, 200.0), p(20.0, 80.0), TANGENT),
            None,
            "lines have no tangent"
        );
        assert_eq!(
            snap_at(&d, p(0.0, 30.0), SnapModes::only(TANGENT)),
            None,
            "no base point"
        );
    }

    #[test]
    fn tangent_on_an_arc_offers_only_points_that_are_on_the_arc() {
        // The upper half of the circle; from the right, only the upper tangent point is on it.
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 30.0, 0.0, PI).into();
        let d = drawing(vec![arc]);
        let found = snap_from(&d, p(50.0, 0.0), p(-20.0, 22.0), TANGENT).unwrap();
        assert!(found.point.distance_to(p(18.0, 24.0)) < 1e-9);
        // From straight below, both tangent points are on the lower half: none.
        assert_eq!(snap_from(&d, p(0.0, -50.0), p(-20.0, 22.0), TANGENT), None);
    }

    #[test]
    fn a_snap_point_under_the_cursor_beats_a_hovered_construction() {
        let d = drawing(vec![line(0.0, 0.0, 200.0, 0.0)]);
        let modes = &[
            SnapMode::Endpoint,
            SnapMode::Midpoint,
            SnapMode::Perpendicular,
        ];
        let base = p(60.0, 90.0);
        // At the end of the line: the endpoint, although the line is hovered.
        assert_eq!(
            snap_from(&d, base, p(198.0, 2.0), modes),
            hit(SnapMode::Endpoint, 200.0, 0.0)
        );
        // Elsewhere on the line: the perpendicular foot.
        assert_eq!(
            snap_from(&d, base, p(150.0, 2.0), modes),
            hit(SnapMode::Perpendicular, 60.0, 0.0)
        );
    }

    #[test]
    fn with_both_constructions_on_the_one_nearer_the_cursor_wins() {
        let d = drawing(vec![Entity::from(CircleEntity::new(p(0.0, 0.0), 30.0))]);
        let both = &[SnapMode::Perpendicular, SnapMode::Tangent];
        let base = p(50.0, 0.0);
        // Feet at (±30, 0); tangent points at (18, ±24).
        let near_the_far_foot = snap_from(&d, base, p(-29.0, 7.0), both).unwrap();
        assert_eq!(near_the_far_foot.mode, SnapMode::Perpendicular);
        // On the circle at 100°: 37 px from the upper tangent point, 46 px from the far foot.
        let on_top = snap_from(&d, base, p(-5.21, 29.54), both).unwrap();
        assert_eq!(on_top.mode, SnapMode::Tangent);
        assert!(on_top.point.distance_to(p(18.0, 24.0)) < 1e-9);
    }

    #[test]
    fn hovering_a_circle_prefers_tangent_to_its_centre() {
        let d = drawing(vec![Entity::from(CircleEntity::new(p(0.0, 0.0), 30.0))]);
        let modes = &[SnapMode::Center, SnapMode::Tangent];
        let found = snap_from(&d, p(50.0, 0.0), p(0.0, 30.0), modes).unwrap();
        assert_eq!(found.mode, SnapMode::Tangent);
        // Without a base point there is no tangent, and the centre is offered as before.
        assert_eq!(
            snap_at(&d, p(0.0, 30.0), SnapModes::only(modes)),
            hit(SnapMode::Center, 0.0, 0.0)
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
                SnapMode::Insertion,
                SnapMode::Center,
                SnapMode::Quadrant,
                SnapMode::Nearest
            ]
        );
        assert!(!modes.is_on(SnapMode::Perpendicular) && !modes.is_on(SnapMode::Tangent));
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
