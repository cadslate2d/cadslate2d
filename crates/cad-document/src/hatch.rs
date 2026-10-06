//! Hatches (DESIGN §34; ADR-0021).
//!
//! A hatch is a region drawn filled: its boundary loops — each a closed
//! path of straight and arc segments — and how it is filled. The fill is
//! not stored: the loops are flattened into rings and the region between
//! them, by the even-odd rule, is cut into triangles whenever the fill is
//! asked for, as a block reference's or a dimension's geometry is. A
//! hatch is an area, not a curve: it is picked by its fill and its
//! boundary, and nothing snaps to, intersects, or trims against it.

use std::f64::consts::TAU;

use cad_core::{Aabb, Point2};
use cad_geometry::{
    Path, Primitive, Transform, flatten_path, ring_contains, signed_area, tessellate,
};

/// How chords follow an arc segment when a loop is flattened: at most
/// this many radians apart, so a full circle is 64 chords.
const CHORD_STEP: f64 = TAU / 64.0;

/// How a hatch is filled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HatchStyle {
    /// Filled solid, in the hatch's colour.
    Solid,
}

/// A filled region.
#[derive(Clone, Debug, PartialEq)]
pub struct HatchEntity {
    /// The boundary: closed paths, in any order and either orientation. By
    /// the even-odd rule a loop inside another is a hole, a loop inside a
    /// hole an island.
    pub loops: Vec<Path>,
    pub style: HatchStyle,
}

/// Why a hatch cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HatchError {
    /// No loops at all.
    NoBoundary,
    /// A loop that is not a region: fewer than three points and no arc, or
    /// no area.
    Degenerate,
    /// A coordinate or bulge that is not a number.
    NotFinite,
}

impl HatchError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> &'static str {
        match self {
            Self::NoBoundary => "A hatch needs a closed boundary.",
            Self::Degenerate => "The boundary encloses no area.",
            Self::NotFinite => "A hatch's points must be numbers.",
        }
    }
}

impl HatchEntity {
    /// A solid hatch of the region `loops` bound. Every loop is taken as
    /// closed.
    pub fn solid(loops: Vec<Path>) -> Self {
        Self {
            loops: loops.into_iter().map(closed).collect(),
            style: HatchStyle::Solid,
        }
    }

    /// Whether the hatch bounds a region.
    pub fn check(&self) -> Result<(), HatchError> {
        if self.loops.is_empty() {
            return Err(HatchError::NoBoundary);
        }
        for path in &self.loops {
            let finite = path.vertices.iter().all(|point| point.is_finite())
                && path.bulges.iter().all(|bulge| bulge.is_finite());
            if !finite {
                return Err(HatchError::NotFinite);
            }
            let curved = path.bulges.iter().any(|bulge| *bulge != 0.0);
            if path.vertices.len() < 2 || (path.vertices.len() < 3 && !curved) {
                return Err(HatchError::Degenerate);
            }
            let ring = flatten_path(path, CHORD_STEP);
            let scale = ring
                .iter()
                .map(|point| point.x.abs() + point.y.abs())
                .fold(1.0, f64::max);
            if signed_area(&ring).abs() <= 1e-12 * scale * scale {
                return Err(HatchError::Degenerate);
            }
        }
        Ok(())
    }

    /// The loops flattened: one ring per loop, arcs as chords.
    pub fn rings(&self) -> Vec<Vec<Point2>> {
        self.loops
            .iter()
            .map(|path| flatten_path(path, CHORD_STEP))
            .collect()
    }

    /// How deep each ring lies: how many of the others contain it. Even
    /// depth is filled, odd is a hole.
    fn depths(rings: &[Vec<Point2>]) -> Vec<usize> {
        rings
            .iter()
            .enumerate()
            .map(|(i, ring)| {
                let Some(probe) = ring.first() else {
                    return 0;
                };
                rings
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| *j != i && ring_contains(other, *probe))
                    .count()
            })
            .collect()
    }

    /// The triangles that fill the region: each outer ring with the holes
    /// one level inside it cut out (ADR-0021).
    pub fn fill(&self) -> Vec<[Point2; 3]> {
        let rings = self.rings();
        let depths = Self::depths(&rings);
        let mut triangles = Vec::new();
        for (i, ring) in rings.iter().enumerate() {
            if !depths[i].is_multiple_of(2) {
                continue;
            }
            let holes: Vec<Vec<Point2>> = rings
                .iter()
                .enumerate()
                .filter(|(j, other)| {
                    *j != i
                        && depths[*j] == depths[i] + 1
                        && other
                            .first()
                            .is_some_and(|probe| ring_contains(ring, *probe))
                })
                .map(|(_, other)| other.clone())
                .collect();
            triangles.extend(tessellate(ring, &holes));
        }
        triangles
    }

    /// The filled area: the outer rings less their holes.
    pub fn area(&self) -> f64 {
        let rings = self.rings();
        let depths = Self::depths(&rings);
        rings
            .iter()
            .zip(&depths)
            .map(|(ring, depth)| {
                let area = signed_area(ring).abs();
                if depth.is_multiple_of(2) { area } else { -area }
            })
            .sum::<f64>()
            .max(0.0)
    }

    /// Whether `point` is in the filled region: inside an odd number of
    /// loops.
    pub fn contains(&self, point: Point2) -> bool {
        let around = self
            .rings()
            .iter()
            .filter(|ring| ring_contains(ring, point))
            .count();
        !around.is_multiple_of(2)
    }

    /// The boundary as lines and arcs.
    pub fn boundary(&self) -> Vec<Primitive> {
        self.loops.iter().flat_map(Path::primitives).collect()
    }

    pub fn bounds(&self) -> Aabb {
        self.boundary()
            .iter()
            .map(Primitive::bounds)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_else(|| {
                let at = self
                    .loops
                    .first()
                    .and_then(|path| path.vertices.first().copied());
                let at = at.unwrap_or(Point2::ORIGIN);
                Aabb::from_points(at, at)
            })
    }

    /// Zero in the fill; outside it, the distance to the nearest boundary.
    pub fn distance_to(&self, point: Point2) -> f64 {
        if self.contains(point) {
            return 0.0;
        }
        self.boundary()
            .iter()
            .map(|primitive| primitive.distance_to_point(point))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether the boundary reaches into `bounds`, or `bounds` lies in the fill.
    pub fn crosses(&self, bounds: &Aabb) -> bool {
        self.boundary()
            .iter()
            .any(|primitive| primitive.intersects_aabb(bounds))
            || self.contains(bounds.center())
    }

    /// Every loop vertex, loop by loop: the grips.
    pub fn vertices(&self) -> Vec<Point2> {
        self.loops
            .iter()
            .flat_map(|path| path.vertices.iter().copied())
            .collect()
    }

    /// The same hatch with vertex `index` (as [`HatchEntity::vertices`]
    /// orders them) at `to`; `None` when there is no such vertex.
    #[must_use]
    pub fn with_vertex(&self, mut index: usize, to: Point2) -> Option<Self> {
        let mut loops = self.loops.clone();
        for path in &mut loops {
            if index < path.vertices.len() {
                path.vertices[index] = to;
                return Some(Self {
                    loops,
                    style: self.style.clone(),
                });
            }
            index -= path.vertices.len();
        }
        None
    }

    pub fn transformed(&self, transform: &Transform) -> Self {
        // Vertices keep their order, so a mirror turns every
        // counter-clockwise arc segment into a clockwise one.
        let flip = if transform.reverses_orientation() {
            -1.0
        } else {
            1.0
        };
        Self {
            loops: self
                .loops
                .iter()
                .map(|path| Path {
                    vertices: path.vertices.iter().map(|v| transform.apply(*v)).collect(),
                    bulges: path.bulges.iter().map(|bulge| bulge * flip).collect(),
                    closed: true,
                })
                .collect(),
            style: self.style.clone(),
        }
    }
}

/// `path`, closed, with a bulge per vertex.
fn closed(mut path: Path) -> Path {
    path.closed = true;
    path.bulges.resize(path.vertices.len(), 0.0);
    path
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{Entity, GripKind, Tables};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn square(x: f64, y: f64, size: f64) -> Path {
        Path {
            vertices: vec![
                p(x, y),
                p(x + size, y),
                p(x + size, y + size),
                p(x, y + size),
            ],
            bulges: Vec::new(),
            closed: false,
        }
    }

    fn circle(center: Point2, radius: f64) -> Path {
        Path {
            vertices: vec![
                p(center.x - radius, center.y),
                p(center.x + radius, center.y),
            ],
            bulges: vec![1.0, 1.0],
            closed: true,
        }
    }

    #[test]
    fn a_square_with_a_round_hole_is_filled_between_them() {
        let hatch = HatchEntity::solid(vec![square(0.0, 0.0, 100.0), circle(p(50.0, 50.0), 10.0)]);
        assert!(hatch.check().is_ok());
        assert!(hatch.loops.iter().all(|path| path.closed));
        assert!(hatch.contains(p(10.0, 10.0)));
        assert!(!hatch.contains(p(50.0, 50.0)), "the hole");
        assert!(!hatch.contains(p(150.0, 50.0)));
        let expected = 10_000.0 - std::f64::consts::PI * 100.0;
        assert!((hatch.area() - expected).abs() < 2.0, "{}", hatch.area());
        let fill = hatch.fill();
        let area: f64 = fill
            .iter()
            .map(|[a, b, c]| ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) / 2.0)
            .sum();
        assert!((area - hatch.area()).abs() < 1e-6);
        assert_eq!(hatch.distance_to(p(10.0, 10.0)), 0.0);
        assert_eq!(hatch.distance_to(p(50.0, 50.0)), 10.0, "to the hole's edge");
        assert_eq!(hatch.distance_to(p(120.0, 50.0)), 20.0);
        assert_eq!(
            hatch.bounds(),
            Aabb::from_points(p(0.0, 0.0), p(100.0, 100.0))
        );
        assert!(
            hatch.crosses(&Aabb::from_points(p(-5.0, 40.0), p(5.0, 60.0))),
            "the edge"
        );
        assert!(
            hatch.crosses(&Aabb::from_points(p(10.0, 10.0), p(20.0, 20.0))),
            "inside"
        );
        assert!(
            !hatch.crosses(&Aabb::from_points(p(45.0, 45.0), p(55.0, 55.0))),
            "in the hole"
        );
        assert!(!hatch.crosses(&Aabb::from_points(p(200.0, 0.0), p(210.0, 10.0))));
        // An island in the hole is filled again.
        let island = HatchEntity::solid(vec![
            square(0.0, 0.0, 100.0),
            square(20.0, 20.0, 60.0),
            square(40.0, 40.0, 20.0),
        ]);
        assert!(island.contains(p(50.0, 50.0)));
        assert!(!island.contains(p(30.0, 30.0)));
        assert!((island.area() - (10_000.0 - 3600.0 + 400.0)).abs() < 1e-9);
    }

    #[test]
    fn what_bounds_no_region_is_refused() {
        assert_eq!(
            HatchEntity::solid(Vec::new()).check(),
            Err(HatchError::NoBoundary)
        );
        let line = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0)],
            bulges: Vec::new(),
            closed: false,
        };
        assert_eq!(
            HatchEntity::solid(vec![line]).check(),
            Err(HatchError::Degenerate)
        );
        let flat = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(20.0, 0.0)],
            bulges: Vec::new(),
            closed: true,
        };
        assert_eq!(
            HatchEntity::solid(vec![flat]).check(),
            Err(HatchError::Degenerate)
        );
        let nan = Path {
            vertices: vec![p(0.0, 0.0), p(f64::NAN, 0.0), p(0.0, 10.0)],
            bulges: Vec::new(),
            closed: true,
        };
        assert_eq!(
            HatchEntity::solid(vec![nan]).check(),
            Err(HatchError::NotFinite)
        );
        assert!(
            HatchEntity::solid(vec![circle(p(0.0, 0.0), 5.0)])
                .check()
                .is_ok(),
            "two bulged vertices"
        );
        assert_eq!(
            HatchError::Degenerate.explain(),
            "The boundary encloses no area."
        );
    }

    #[test]
    fn grips_are_the_loop_vertices_and_transforms_keep_the_fill() {
        let hatch = HatchEntity::solid(vec![square(0.0, 0.0, 10.0), circle(p(5.0, 5.0), 1.0)]);
        assert_eq!(hatch.vertices().len(), 6);
        let moved = hatch.with_vertex(2, p(12.0, 12.0)).unwrap();
        assert_eq!(moved.loops[0].vertices[2], p(12.0, 12.0));
        assert_eq!(
            hatch.with_vertex(4, p(0.0, 5.0)).unwrap().loops[1].vertices[0],
            p(0.0, 5.0)
        );
        assert_eq!(hatch.with_vertex(6, p(0.0, 0.0)), None);
        let entity = Entity::from(hatch.clone());
        assert_eq!(entity.grips().len(), 6);
        assert!(
            entity
                .grips()
                .iter()
                .all(|grip| grip.kind == GripKind::Vertex)
        );
        assert!(
            entity.grip_moved(0, p(10.0, 0.0)).is_none(),
            "onto its neighbour"
        );
        assert!(entity.grip_moved(0, p(-5.0, -5.0)).is_some());
        // Mirrored, the hole is still a hole: the arcs flip with the order.
        let mirrored = hatch.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        assert!(mirrored.contains(p(-2.0, 2.0)));
        assert!(!mirrored.contains(p(-5.0, 5.0)));
        assert!((mirrored.area() - hatch.area()).abs() < 1e-9);
        let scaled = hatch.transformed(&Transform::scaling(p(0.0, 0.0), 2.0).unwrap());
        assert!((scaled.area() - 4.0 * hatch.area()).abs() < 1e-6);
        // Through the entity: picked by its fill, not a curve, no snap points.
        assert_eq!(entity.distance_to(p(2.0, 2.0), Tables::none()), 0.0);
        assert!(entity.primitives(Tables::none()).is_empty());
        assert_eq!(entity.kind_name(), "hatch");
    }

    fn star(center: Point2, count: usize, radii: &[f64]) -> Path {
        Path {
            vertices: (0..count)
                .map(|i| {
                    let angle = i as f64 / count as f64 * TAU;
                    center + cad_core::Vec2::from_polar(radii[i % radii.len()], angle)
                })
                .collect(),
            bulges: Vec::new(),
            closed: true,
        }
    }

    proptest! {
        /// The fill lies inside the boundary and nowhere else: every
        /// triangle's centroid is in the region, and the triangles add up
        /// to its area.
        #[test]
        fn the_fill_lies_inside_the_boundary(
            count in 3usize..30,
            radii in prop::collection::vec(40.0f64..100.0, 1..6),
            hole_radii in prop::collection::vec(3.0f64..12.0, 1..4),
            cx in -500.0f64..500.0, cy in -500.0f64..500.0,
        ) {
            let center = p(cx, cy);
            let hatch = HatchEntity::solid(vec![
                star(center, count, &radii),
                star(center + cad_core::Vec2::new(3.0, -2.0), 7, &hole_radii),
            ]);
            prop_assert!(hatch.check().is_ok());
            let fill = hatch.fill();
            let mut area = 0.0;
            for [a, b, c] in &fill {
                let twice = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                prop_assert!(twice > 0.0, "a triangle with area, counter-clockwise");
                area += twice / 2.0;
                let centroid = p((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
                prop_assert!(hatch.contains(centroid), "{centroid:?} is not in the hatch");
            }
            prop_assert!((area - hatch.area()).abs() < 1e-6 * hatch.area(), "{area} vs {}", hatch.area());
            // Moving the hatch moves the fill.
            let moved = hatch.transformed(&Transform::translation(cad_core::Vec2::new(10.0, 20.0)));
            prop_assert!((moved.area() - hatch.area()).abs() < 1e-6 * hatch.area());
            prop_assert!(moved.contains(center + cad_core::Vec2::new(10.0, 20.0) + cad_core::Vec2::new(30.0, 0.0)) == hatch.contains(center + cad_core::Vec2::new(30.0, 0.0)));
        }
    }
}
