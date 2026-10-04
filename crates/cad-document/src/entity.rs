//! Semantic entity types (DESIGN §11).

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{
    Arc, Circle, Path, Primitive, Segment, Transform, is_straight, polyline_primitives,
};

/// A straight line between two points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEntity {
    pub start: Point2,
    pub end: Point2,
}

impl LineEntity {
    pub const fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }

    pub fn segment(&self) -> Segment {
        Segment::new(self.start, self.end)
    }
}

/// A full circle. It stays a circle in the document; only the renderer may
/// approximate it (DESIGN §11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleEntity {
    pub center: Point2,
    pub radius: f64,
}

impl CircleEntity {
    pub const fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn circle(&self) -> Circle {
        Circle::new(self.center, self.radius)
    }
}

/// A circular arc, counter-clockwise from `start_angle` to `end_angle`
/// (radians from +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcEntity {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl ArcEntity {
    pub const fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            center,
            radius,
            start_angle,
            end_angle,
        }
    }

    pub fn arc(&self) -> Arc {
        Arc::from_angles(self.center, self.radius, self.start_angle, self.end_angle)
    }
}

impl From<Arc> for ArcEntity {
    fn from(arc: Arc) -> Self {
        Self::new(arc.center, arc.radius, arc.start, arc.end())
    }
}

/// A chain of segments through at least two vertices, optionally closed back
/// to the first. A segment is straight unless the vertex it starts at has a
/// *bulge*, which makes it a circular arc (see `cad_geometry::polyline` for
/// the convention: `tan(included angle / 4)`, positive counter-clockwise).
///
/// Bulges are stored only as far as the last non-zero one, so two polylines
/// with the same shape always compare equal, and an all-straight polyline
/// carries no bulge data at all.
#[derive(Clone, Debug, PartialEq)]
pub struct PolylineEntity {
    pub vertices: Vec<Point2>,
    /// One per vertex, for the segment starting there; missing ones are zero.
    bulges: Vec<f64>,
    pub closed: bool,
}

impl PolylineEntity {
    /// A polyline of straight segments.
    pub fn new(vertices: Vec<Point2>, closed: bool) -> Self {
        Self {
            vertices,
            bulges: Vec::new(),
            closed,
        }
    }

    /// A polyline whose segment from vertex `i` has bulge `bulges[i]`.
    /// Missing bulges are zero; so are those of segments that do not exist
    /// (the last vertex of an open polyline) and any that are not numbers.
    pub fn with_bulges(vertices: Vec<Point2>, mut bulges: Vec<f64>, closed: bool) -> Self {
        let segments = cad_geometry::polyline_spans(vertices.len(), closed).count();
        bulges.truncate(segments);
        for bulge in &mut bulges {
            if is_straight(*bulge) {
                *bulge = 0.0;
            }
        }
        let used = bulges
            .iter()
            .rposition(|bulge| bulge.abs() > 0.0)
            .map_or(0, |last| last + 1);
        bulges.truncate(used);
        Self {
            vertices,
            bulges,
            closed,
        }
    }

    /// The bulge of the segment that starts at vertex `index`.
    pub fn bulge(&self, index: usize) -> f64 {
        self.bulges.get(index).copied().unwrap_or(0.0)
    }

    /// One bulge per vertex.
    pub fn bulges(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.vertices.len()).map(|index| self.bulge(index))
    }

    /// True when at least one segment is an arc.
    pub fn has_arcs(&self) -> bool {
        !self.bulges.is_empty()
    }

    /// The polyline as a path that can be measured along and cut.
    pub fn path(&self) -> Path {
        Path {
            vertices: self.vertices.clone(),
            bulges: self.bulges().collect(),
            closed: self.closed,
        }
    }

    /// The segments in order — straight segments and arcs — including the
    /// closing one when closed.
    pub fn primitives(&self) -> impl Iterator<Item = Primitive> + '_ {
        polyline_primitives(&self.vertices, |index| self.bulge(index), self.closed)
    }
}

impl From<Path> for PolylineEntity {
    fn from(path: Path) -> Self {
        Self::with_bulges(path.vertices, path.bulges, path.closed)
    }
}

/// Every kind of entity a drawing can hold.
///
/// Deliberately *not* `#[non_exhaustive]`: when a later milestone adds a
/// variant (polyline, text, …) the compiler must point at every match in the
/// workspace — rendering, picking, DXF — rather than letting a wildcard arm
/// silently ignore the new entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Line(LineEntity),
    Circle(CircleEntity),
    Arc(ArcEntity),
    Polyline(PolylineEntity),
}

impl Entity {
    /// World-space bounding box, used by the spatial index.
    pub fn bounds(&self) -> Aabb {
        match self {
            Entity::Line(line) => line.segment().bounds(),
            Entity::Circle(circle) => circle.circle().bounds(),
            Entity::Arc(arc) => arc.arc().bounds(),
            // Arc segments can reach beyond the vertices. Every polyline in a
            // drawing has segments; one without bounds nothing useful.
            Entity::Polyline(polyline) => polyline
                .primitives()
                .map(|primitive| primitive.bounds())
                .reduce(|a, b| a.union(&b))
                .unwrap_or_else(|| Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN)),
        }
    }

    /// Exact distance from `point` to the entity's geometry, used for picking.
    pub fn distance_to(&self, point: Point2) -> f64 {
        match self {
            Entity::Line(line) => line.segment().distance_to_point(point),
            Entity::Circle(circle) => circle.circle().distance_to_point(point),
            Entity::Arc(arc) => arc.arc().distance_to_point(point),
            Entity::Polyline(polyline) => polyline
                .primitives()
                .map(|primitive| primitive.distance_to_point(point))
                .fold(f64::INFINITY, f64::min),
        }
    }

    /// The entity as lines, circles, and arcs: the common currency of
    /// intersection, snapping, and trimming.
    pub fn primitives(&self) -> Vec<Primitive> {
        match self {
            Entity::Line(line) => vec![Primitive::Segment(line.segment())],
            Entity::Circle(circle) => vec![Primitive::Circle(circle.circle())],
            Entity::Arc(arc) => vec![Primitive::Arc(arc.arc())],
            Entity::Polyline(polyline) => polyline.primitives().collect(),
        }
    }

    /// Every point where this entity's geometry meets `other`'s.
    pub fn intersections(&self, other: &Entity) -> Vec<Point2> {
        let theirs = other.primitives();
        self.primitives()
            .iter()
            .flat_map(|mine| theirs.iter().map(move |their| mine.intersect(their)))
            .flat_map(|hits| hits.as_slice().to_vec())
            .collect()
    }

    /// True when the whole entity lies within `bounds` (window selection).
    pub fn is_inside(&self, bounds: &Aabb) -> bool {
        bounds.contains(&self.bounds())
    }

    /// True when any part of the entity's geometry lies within `bounds`
    /// (crossing selection). Interiors do not count.
    pub fn crosses(&self, bounds: &Aabb) -> bool {
        match self {
            Entity::Line(line) => line.segment().intersects_aabb(bounds),
            Entity::Circle(circle) => circle.circle().intersects_aabb(bounds),
            Entity::Arc(arc) => arc.arc().intersects_aabb(bounds),
            Entity::Polyline(polyline) => polyline
                .primitives()
                .any(|primitive| primitive.intersects_aabb(bounds)),
        }
    }

    /// A copy of the entity moved by `offset`. Exact: coordinates are added
    /// to, and nothing else changes.
    pub fn translated(&self, offset: Vec2) -> Self {
        self.transformed(&Transform::translation(offset))
    }

    /// A copy of the entity moved, rotated, uniformly scaled, or mirrored.
    /// Every kind of entity stays its kind (DESIGN §22).
    pub fn transformed(&self, transform: &Transform) -> Self {
        match self {
            Entity::Line(line) => {
                LineEntity::new(transform.apply(line.start), transform.apply(line.end)).into()
            }
            Entity::Circle(circle) => CircleEntity::new(
                transform.apply(circle.center),
                circle.radius * transform.scale_factor(),
            )
            .into(),
            Entity::Arc(arc) => {
                // Arcs run counter-clockwise from start to end. A mirror
                // reverses direction, so the two ends trade places.
                let (start, end) = if transform.reverses_orientation() {
                    (arc.end_angle, arc.start_angle)
                } else {
                    (arc.start_angle, arc.end_angle)
                };
                ArcEntity::new(
                    transform.apply(arc.center),
                    arc.radius * transform.scale_factor(),
                    transform.apply_angle(start),
                    transform.apply_angle(end),
                )
                .into()
            }
            Entity::Polyline(polyline) => {
                // Vertices keep their order, so a mirror turns every
                // counter-clockwise arc segment into a clockwise one.
                let flip = if transform.reverses_orientation() {
                    -1.0
                } else {
                    1.0
                };
                PolylineEntity::with_bulges(
                    polyline
                        .vertices
                        .iter()
                        .map(|vertex| transform.apply(*vertex))
                        .collect(),
                    polyline.bulges().map(|bulge| bulge * flip).collect(),
                    polyline.closed,
                )
                .into()
            }
        }
    }

    pub fn as_line(&self) -> Option<&LineEntity> {
        match self {
            Entity::Line(line) => Some(line),
            _ => None,
        }
    }
}

impl From<LineEntity> for Entity {
    fn from(line: LineEntity) -> Self {
        Entity::Line(line)
    }
}

impl From<CircleEntity> for Entity {
    fn from(circle: CircleEntity) -> Self {
        Entity::Circle(circle)
    }
}

impl From<PolylineEntity> for Entity {
    fn from(polyline: PolylineEntity) -> Self {
        Entity::Polyline(polyline)
    }
}

impl From<ArcEntity> for Entity {
    fn from(arc: ArcEntity) -> Self {
        Entity::Arc(arc)
    }
}

/// An immutable entity payload shared between the store and undo history, so
/// history never deep-copies large entities (DESIGN §19.4).
pub type SharedEntity = std::sync::Arc<Entity>;

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    fn line() -> Entity {
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)).into()
    }

    #[test]
    fn line_bounds_cover_its_endpoints() {
        let bounds = line().bounds();
        assert_eq!(bounds.min(), Point2::new(0.0, 0.0));
        assert_eq!(bounds.max(), Point2::new(10.0, 0.0));
    }

    #[test]
    fn line_distance_is_point_to_segment_distance() {
        assert_eq!(line().distance_to(Point2::new(5.0, 2.0)), 2.0);
    }

    #[test]
    fn a_circle_is_bounded_and_picked_by_its_curve() {
        let circle: Entity = CircleEntity::new(Point2::new(10.0, 10.0), 4.0).into();
        assert_eq!(
            circle.bounds(),
            Aabb::from_points(Point2::new(6.0, 6.0), Point2::new(14.0, 14.0))
        );
        assert_eq!(
            circle.distance_to(Point2::new(10.0, 10.0)),
            4.0,
            "the interior is not the circle"
        );
        assert_eq!(circle.distance_to(Point2::new(15.0, 10.0)), 1.0);
    }

    #[test]
    fn an_arc_runs_counter_clockwise_between_its_angles() {
        let arc = ArcEntity::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2);
        let entity: Entity = arc.into();
        assert!(entity.distance_to(Point2::new(0.0, 12.0)) - 2.0 < 1e-12);
        // Opposite the arc: far from it, although on its circle.
        assert!(entity.distance_to(Point2::new(-10.0, 0.0)) > 14.0);
        assert!((arc.arc().sweep - FRAC_PI_2).abs() < 1e-12);

        // The geometry type and the entity agree in both directions.
        let back: ArcEntity = Arc::new(Point2::ORIGIN, 10.0, PI, FRAC_PI_2).into();
        assert!((back.start_angle - PI).abs() < 1e-12 && (back.end_angle - 1.5 * PI).abs() < 1e-12);
    }

    #[test]
    fn a_polyline_is_bounded_by_its_vertices_and_picked_on_its_segments() {
        let rectangle: Entity = PolylineEntity::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 5.0),
                Point2::new(0.0, 5.0),
            ],
            true,
        )
        .into();
        assert_eq!(
            rectangle.bounds(),
            Aabb::from_points(Point2::new(0.0, 0.0), Point2::new(10.0, 5.0))
        );
        assert_eq!(
            rectangle.distance_to(Point2::new(5.0, 2.5)),
            2.5,
            "the interior is not the polyline"
        );
        assert_eq!(
            rectangle.distance_to(Point2::new(-1.0, 2.5)),
            1.0,
            "the closing segment counts"
        );
    }

    fn one_of_each() -> Vec<Entity> {
        vec![
            LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)).into(),
            CircleEntity::new(Point2::new(5.0, 5.0), 3.0).into(),
            ArcEntity::new(Point2::new(5.0, 5.0), 4.0, 0.0, std::f64::consts::PI).into(),
            PolylineEntity::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 4.0),
                ],
                true,
            )
            .into(),
        ]
    }

    /// Sample points along an entity: enough to tell whether two entities
    /// trace the same curve.
    fn samples(entity: &Entity) -> Vec<Point2> {
        match entity {
            Entity::Line(line) => vec![line.start, line.end],
            Entity::Circle(circle) => vec![circle.center],
            Entity::Arc(arc) => {
                let arc = arc.arc();
                vec![arc.start_point(), arc.mid_point(), arc.end_point()]
            }
            Entity::Polyline(polyline) => polyline.vertices.clone(),
        }
    }

    fn same_curve(a: &Entity, b: &Entity) -> bool {
        std::mem::discriminant(a) == std::mem::discriminant(b)
            && samples(a)
                .iter()
                .zip(samples(b))
                .all(|(a, b)| a.distance_to(b) < 1e-9)
            && (a.bounds().max().distance_to(b.bounds().max()) < 1e-9)
    }

    #[test]
    fn every_kind_keeps_its_kind_under_every_transform() {
        let about = Point2::new(3.0, -2.0);
        let transforms = [
            Transform::translation(Vec2::new(7.0, 1.0)),
            Transform::rotation(about, 37.0),
            Transform::scaling(about, 2.5).unwrap(),
            Transform::mirror(about, Point2::new(8.0, 5.0)).unwrap(),
        ];
        for entity in one_of_each() {
            for transform in &transforms {
                let image = entity.transformed(transform);
                assert_eq!(
                    std::mem::discriminant(&image),
                    std::mem::discriminant(&entity)
                );
                // Every sample point of the original maps onto the image.
                for point in samples(&entity) {
                    if matches!(entity, Entity::Circle(_)) {
                        continue; // the centre is not on the curve
                    }
                    let distance = image.distance_to(transform.apply(point));
                    assert!(
                        distance < 1e-9,
                        "{entity:?} under {transform:?}: {distance}"
                    );
                }
            }
        }
    }

    /// #93: rotating by 360° reproduces the original geometry within tolerance.
    #[test]
    fn a_full_turn_reproduces_every_entity() {
        let about = Point2::new(-4.0, 9.0);
        for entity in one_of_each() {
            let full = entity.transformed(&Transform::rotation(about, 360.0));
            assert!(same_curve(&full, &entity), "{entity:?} became {full:?}");
            let mut stepped = entity.clone();
            for _ in 0..8 {
                stepped = stepped.transformed(&Transform::rotation(about, 45.0));
            }
            assert!(
                same_curve(&stepped, &entity),
                "{entity:?} became {stepped:?}"
            );
        }
    }

    /// #95: mirroring twice across the same line reproduces the original.
    #[test]
    fn mirroring_twice_reproduces_every_entity() {
        let mirror = Transform::mirror(Point2::new(1.0, 2.0), Point2::new(6.0, -3.0)).unwrap();
        for entity in one_of_each() {
            let twice = entity.transformed(&mirror).transformed(&mirror);
            assert!(same_curve(&twice, &entity), "{entity:?} became {twice:?}");
        }
    }

    #[test]
    fn a_rotated_arc_turns_both_angles() {
        let arc: Entity = ArcEntity::new(Point2::new(10.0, 0.0), 5.0, 0.0, FRAC_PI_2).into();
        let Entity::Arc(image) = arc.transformed(&Transform::rotation(Point2::ORIGIN, 90.0)) else {
            panic!("an arc stays an arc");
        };
        assert_eq!(image.center, Point2::new(0.0, 10.0));
        assert_eq!(image.radius, 5.0);
        assert!((image.start_angle - FRAC_PI_2).abs() < 1e-15);
        assert!((image.end_angle - PI).abs() < 1e-15);
    }

    /// #95: a mirrored arc keeps a valid counter-clockwise representation.
    #[test]
    fn a_mirrored_arc_swaps_its_ends_and_stays_counter_clockwise() {
        // A quarter arc from (10,0) up to (0,10), mirrored across the Y axis.
        let arc: Entity = ArcEntity::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2).into();
        let mirror = Transform::mirror(Point2::ORIGIN, Point2::new(0.0, 1.0)).unwrap();
        let Entity::Arc(image) = arc.transformed(&mirror) else {
            panic!("an arc stays an arc");
        };
        // Counter-clockwise from (0,10) to (-10,0): the second quadrant.
        assert!((image.start_angle - FRAC_PI_2).abs() < 1e-15);
        assert!((image.end_angle - PI).abs() < 1e-15);
        assert!(
            (image.arc().sweep - FRAC_PI_2).abs() < 1e-15,
            "not the three-quarter arc"
        );
    }

    #[test]
    fn scaling_scales_radii_and_keeps_arc_angles() {
        let scale = Transform::scaling(Point2::ORIGIN, 3.0).unwrap();
        let circle: Entity = CircleEntity::new(Point2::new(1.0, 2.0), 4.0).into();
        assert_eq!(
            circle.transformed(&scale),
            CircleEntity::new(Point2::new(3.0, 6.0), 12.0).into()
        );
        let arc: Entity = ArcEntity::new(Point2::new(1.0, 2.0), 4.0, 0.3, 2.1).into();
        assert_eq!(
            arc.transformed(&scale),
            ArcEntity::new(Point2::new(3.0, 6.0), 12.0, 0.3, 2.1).into()
        );
    }

    // ── polyline arc segments (#114) ──

    /// A slot from (0,0) to (10,4): two straight sides and two semicircular
    /// ends of radius 2, bulging out to x = -2 and x = 12.
    fn slot() -> PolylineEntity {
        PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        )
    }

    #[test]
    fn bulges_have_one_canonical_form() {
        let corners = || {
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ]
        };
        let plain = PolylineEntity::new(corners(), false);
        assert!(!plain.has_arcs());
        assert_eq!(plain.bulges().collect::<Vec<_>>(), vec![0.0, 0.0, 0.0]);

        // Zeros, a bulge on a segment that does not exist, and non-numbers
        // all mean "straight", and compare equal to a plain polyline.
        for bulges in [
            vec![],
            vec![0.0, 0.0],
            vec![0.0, 0.0, 0.5],
            vec![f64::NAN, 1e-300],
        ] {
            assert_eq!(
                PolylineEntity::with_bulges(corners(), bulges.clone(), false),
                plain,
                "{bulges:?}"
            );
        }
        // Closed, the last vertex does start a segment.
        let closed = PolylineEntity::with_bulges(corners(), vec![0.0, 0.0, 0.5], true);
        assert!(closed.has_arcs());
        assert_eq!(closed.bulge(2), 0.5);
        assert_eq!(closed.bulge(99), 0.0);
        // Trailing zeros are not stored, so equal shapes are equal values.
        assert_eq!(
            PolylineEntity::with_bulges(corners(), vec![0.5, 0.0, 0.0], true),
            PolylineEntity::with_bulges(corners(), vec![0.5], true)
        );
    }

    #[test]
    fn an_arc_segment_reaches_beyond_the_vertices() {
        let slot = Entity::from(slot());
        assert_eq!(
            slot.bounds(),
            Aabb::from_points(Point2::new(-2.0, 0.0), Point2::new(12.0, 4.0))
        );
        // On the right end's arc, 2 from its centre (10, 2).
        assert!(slot.distance_to(Point2::new(12.0, 2.0)) < 1e-12);
        // The chord between vertices 1 and 2 is not part of the polyline.
        assert!((slot.distance_to(Point2::new(10.0, 2.0)) - 2.0).abs() < 1e-12);
        assert_eq!(slot.primitives().len(), 4);
        assert_eq!(
            slot.primitives()
                .iter()
                .filter(|primitive| matches!(primitive, Primitive::Arc(_)))
                .count(),
            2
        );
    }

    #[test]
    fn selection_boxes_see_the_arc_not_the_chord() {
        let slot = Entity::from(slot());
        // A box over the bulge of the right end only.
        let over_the_bulge = Aabb::from_points(Point2::new(11.5, 1.5), Point2::new(13.0, 2.5));
        assert!(slot.crosses(&over_the_bulge));
        // A box over the chord, inside the bulge: nothing is there.
        let over_the_chord = Aabb::from_points(Point2::new(9.8, 1.5), Point2::new(10.4, 2.5));
        assert!(!slot.crosses(&over_the_chord));
        // Window selection needs the bulges inside too.
        let vertices_only = Aabb::from_points(Point2::new(-0.5, -0.5), Point2::new(10.5, 4.5));
        assert!(!slot.is_inside(&vertices_only));
        assert!(slot.is_inside(&vertices_only.expanded(2.0)));
    }

    #[test]
    fn other_entities_intersect_the_arc_segments() {
        let slot = Entity::from(slot());
        // A horizontal line through the middle crosses both semicircular ends.
        let through: Entity =
            LineEntity::new(Point2::new(-20.0, 2.0), Point2::new(20.0, 2.0)).into();
        let mut hits = slot.intersections(&through);
        hits.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(hits.len(), 2);
        assert!(hits[0].distance_to(Point2::new(-2.0, 2.0)) < 1e-9);
        assert!(hits[1].distance_to(Point2::new(12.0, 2.0)) < 1e-9);
    }

    #[test]
    fn a_mirror_negates_bulges_and_other_transforms_keep_them() {
        let slot = Entity::from(slot());
        let bulges = |entity: &Entity| match entity {
            Entity::Polyline(polyline) => polyline.bulges().collect::<Vec<_>>(),
            other => panic!("expected a polyline, got {other:?}"),
        };
        let about = Point2::new(3.0, -2.0);
        for kept in [
            Transform::translation(Vec2::new(7.0, 1.0)),
            Transform::rotation(about, 37.0),
            Transform::scaling(about, 2.5).unwrap(),
        ] {
            assert_eq!(bulges(&slot.transformed(&kept)), vec![0.0, 1.0, 0.0, 1.0]);
        }
        let mirror = Transform::mirror(about, Point2::new(8.0, 5.0)).unwrap();
        assert_eq!(
            bulges(&slot.transformed(&mirror)),
            vec![0.0, -1.0, 0.0, -1.0]
        );
        assert_eq!(
            bulges(&slot.transformed(&mirror).transformed(&mirror)),
            vec![0.0, 1.0, 0.0, 1.0]
        );
        // A move is still exact.
        let offset = Vec2::new(100.0, -50.0);
        assert_eq!(slot.translated(offset).translated(-offset), slot);
    }

    mod bulged_polylines {
        use proptest::prelude::*;

        use super::*;

        fn coordinate() -> impl Strategy<Value = f64> {
            -500.0..500f64
        }

        fn point() -> impl Strategy<Value = Point2> {
            (coordinate(), coordinate()).prop_map(|(x, y)| Point2::new(x, y))
        }

        /// A polyline of three to six well-separated vertices with a mix of
        /// straight and arc segments.
        fn polyline() -> impl Strategy<Value = PolylineEntity> {
            (3..7usize, any::<bool>()).prop_flat_map(|(count, closed)| {
                (
                    proptest::collection::vec(point(), count),
                    proptest::collection::vec(prop_oneof![Just(0.0), -3.0..3.0f64], count),
                )
                    .prop_filter("vertices apart", |(vertices, _)| {
                        vertices.iter().enumerate().all(|(index, a)| {
                            vertices[index + 1..]
                                .iter()
                                .all(|b| a.distance_to(*b) > 1.0)
                        })
                    })
                    .prop_map(move |(vertices, bulges)| {
                        PolylineEntity::with_bulges(vertices, bulges, closed)
                    })
            })
        }

        /// Points along every segment: both ends and the middle.
        fn samples(polyline: &PolylineEntity) -> Vec<Point2> {
            polyline
                .primitives()
                .flat_map(|primitive| match primitive {
                    Primitive::Segment(segment) => {
                        vec![
                            segment.start,
                            segment.start + (segment.end - segment.start) * 0.5,
                            segment.end,
                        ]
                    }
                    Primitive::Arc(arc) => {
                        vec![arc.start_point(), arc.mid_point(), arc.end_point()]
                    }
                    Primitive::Circle(circle) => vec![circle.center],
                })
                .collect()
        }

        proptest! {
            /// #114: transforms preserve the shape — the image of every point
            /// of the polyline lies on the image of the polyline.
            #[test]
            fn transforms_carry_every_point_onto_the_image(
                polyline in polyline(), about in point(), other in point(),
                degrees in -360.0..360f64, factor in 0.2..5f64,
            ) {
                prop_assume!(about.distance_to(other) > 1.0);
                let entity = Entity::from(polyline.clone());
                let transforms = [
                    Transform::translation(other - about),
                    Transform::rotation(about, degrees),
                    Transform::scaling(about, factor).unwrap(),
                    Transform::mirror(about, other).unwrap(),
                ];
                for transform in transforms {
                    let image = entity.transformed(&transform);
                    let Entity::Polyline(image_polyline) = &image else {
                        return Err(TestCaseError::fail("a polyline stays a polyline"));
                    };
                    prop_assert_eq!(image_polyline.vertices.len(), polyline.vertices.len());
                    prop_assert_eq!(image.primitives().len(), entity.primitives().len());
                    let size = image.bounds().min().distance_to(image.bounds().max());
                    for point in samples(&polyline) {
                        let off = image.distance_to(transform.apply(point));
                        prop_assert!(off < 1e-6 * (1.0 + size), "{:?}: off by {}", transform, off);
                    }
                }
            }

            /// Mirroring twice gives back the same polyline.
            #[test]
            fn a_double_mirror_restores_the_polyline(polyline in polyline(), about in point(), other in point()) {
                prop_assume!(about.distance_to(other) > 1.0);
                let mirror = Transform::mirror(about, other).unwrap();
                let entity = Entity::from(polyline.clone());
                let Entity::Polyline(twice) = entity.transformed(&mirror).transformed(&mirror) else {
                    return Err(TestCaseError::fail("a polyline stays a polyline"));
                };
                prop_assert_eq!(twice.bulges().collect::<Vec<_>>(), polyline.bulges().collect::<Vec<_>>());
                for (a, b) in twice.vertices.iter().zip(&polyline.vertices) {
                    prop_assert!(a.distance_to(*b) < 1e-6);
                }
            }

            /// The bounding box holds every sample point, arcs included.
            #[test]
            fn bounds_contain_the_whole_polyline(polyline in polyline()) {
                let entity = Entity::from(polyline.clone());
                let bounds = entity.bounds().expanded(1e-6);
                for point in samples(&polyline) {
                    prop_assert!(bounds.contains_point(point), "{:?} outside {:?}", point, bounds);
                    prop_assert!(entity.distance_to(point) < 1e-6);
                }
            }
        }
    }

    #[test]
    fn translation_moves_every_kind_and_keeps_its_shape() {
        let offset = Vec2::new(100.0, -50.0);
        for entity in one_of_each() {
            let moved = entity.translated(offset);
            assert_eq!(
                moved.bounds().min(),
                entity.bounds().min() + offset,
                "{entity:?}"
            );
            assert_eq!(
                moved.bounds().max(),
                entity.bounds().max() + offset,
                "{entity:?}"
            );
            assert_eq!(
                moved.translated(-offset),
                entity,
                "moving back restores it exactly"
            );
        }
    }

    #[test]
    fn window_selection_needs_the_whole_entity_and_crossing_any_part() {
        let around_everything =
            Aabb::from_points(Point2::new(-50.0, -50.0), Point2::new(50.0, 50.0));
        let far_away = Aabb::from_points(Point2::new(500.0, 500.0), Point2::new(600.0, 600.0));
        for entity in one_of_each() {
            assert!(
                entity.is_inside(&around_everything) && entity.crosses(&around_everything),
                "{entity:?}"
            );
            assert!(
                !entity.is_inside(&far_away) && !entity.crosses(&far_away),
                "{entity:?}"
            );
        }

        // A box over the right half of the line: crossing, but not inside.
        let line = &one_of_each()[0];
        let right_half = Aabb::from_points(Point2::new(5.0, -1.0), Point2::new(20.0, 1.0));
        assert!(line.crosses(&right_half) && !line.is_inside(&right_half));

        // A box inside the circle touches nothing.
        let circle = &one_of_each()[1];
        let inside_disc = Aabb::from_points(Point2::new(4.5, 4.5), Point2::new(5.5, 5.5));
        assert!(!circle.crosses(&inside_disc));
    }

    #[test]
    fn entities_intersect_through_their_primitives() {
        let all = one_of_each();
        let (line, circle, triangle) = (&all[0], &all[1], &all[3]);
        assert_eq!(
            all.iter()
                .map(|entity| entity.primitives().len())
                .collect::<Vec<_>>(),
            vec![1, 1, 1, 3]
        );

        // A vertical line through the circle's centre crosses it twice.
        let vertical: Entity =
            LineEntity::new(Point2::new(5.0, -10.0), Point2::new(5.0, 20.0)).into();
        let mut hits = vertical.intersections(circle);
        hits.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert_eq!(hits.len(), 2);
        assert!(
            hits[0].distance_to(Point2::new(5.0, 2.0)) < 1e-9
                && hits[1].distance_to(Point2::new(5.0, 8.0)) < 1e-9
        );

        // It also crosses the bottom line once.
        assert_eq!(vertical.intersections(line).len(), 1);
        // A vertical through the closed triangle crosses its base and its hypotenuse.
        let through: Entity =
            LineEntity::new(Point2::new(2.0, -10.0), Point2::new(2.0, 20.0)).into();
        assert_eq!(triangle.intersections(&through).len(), 2);
        // Symmetric.
        assert_eq!(circle.intersections(&vertical).len(), 2);
    }

    #[test]
    fn as_line_distinguishes_entity_kinds() {
        assert!(line().as_line().is_some());
        assert!(
            Entity::from(CircleEntity::new(Point2::ORIGIN, 1.0))
                .as_line()
                .is_none()
        );
    }
}
