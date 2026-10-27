//! Hatches (DESIGN §34; ADR-0021).
//!
//! A hatch is a region drawn filled: its boundary loops — each a closed
//! path of straight and arc segments — and how it is filled. The fill is
//! not stored: the loops are flattened into rings and the region between
//! them, by the even-odd rule, is cut into triangles whenever the fill is
//! asked for, as a block reference's or a dimension's geometry is. A
//! hatch is an area, not a curve: it is picked by its fill and its
//! boundary, and nothing snaps to, intersects, or trims against it.
//!
//! A pattern hatch (#168) is filled with the lines of a pattern from the
//! table instead ([`crate::hatch_patterns`]): each family's lines clipped
//! to the rings by the same even-odd rule, dashed in drawing units, placed
//! at the hatch's angle, scale, and origin — and likewise computed
//! whenever asked for.

use std::f64::consts::TAU;

use cad_core::{Aabb, Point2};
use cad_geometry::{
    LineFamily, Path, Primitive, Segment, Transform, flatten_path, pattern_lines, ring_contains,
    signed_area, tessellate,
};

use crate::hatch_pattern::{HatchPattern, hatch_pattern};

/// How chords follow an arc segment when a loop is flattened: at most
/// this many radians apart, so a full circle is 64 chords.
const CHORD_STEP: f64 = TAU / 64.0;
/// The most pattern pieces one hatch may be drawn with: past it, the
/// pattern is too fine for the region at its scale.
pub const MAX_PATTERN_PIECES: usize = 200_000;

/// How a hatch is filled.
#[derive(Clone, Debug, PartialEq)]
pub enum HatchStyle {
    /// Filled solid, in the hatch's colour.
    Solid,
    /// Filled with the lines of a pattern from the table, by name, turned
    /// by `angle` (radians), scaled by `scale`, from `origin`.
    Pattern {
        name: String,
        angle: f64,
        scale: f64,
        origin: Point2,
    },
}

impl HatchStyle {
    /// A style by pattern name: `SOLID` is the solid fill.
    pub fn named(name: &str, angle: f64, scale: f64) -> Self {
        if name.eq_ignore_ascii_case(HatchPattern::SOLID) {
            Self::Solid
        } else {
            Self::Pattern {
                name: name.to_owned(),
                angle,
                scale,
                origin: Point2::ORIGIN,
            }
        }
    }

    /// The pattern's name; `SOLID` for the solid fill.
    pub fn name(&self) -> &str {
        match self {
            Self::Solid => HatchPattern::SOLID,
            Self::Pattern { name, .. } => name,
        }
    }
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
    /// A pattern the table does not have.
    UnknownPattern,
    /// A pattern scale that is not positive, or an angle that is not a number.
    BadScale,
    /// More pattern lines than a hatch is drawn with: too fine a scale for
    /// the region.
    TooDense,
}

impl HatchError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> &'static str {
        match self {
            Self::NoBoundary => "A hatch needs a closed boundary.",
            Self::Degenerate => "The boundary encloses no area.",
            Self::NotFinite => "A hatch's points must be numbers.",
            Self::UnknownPattern => "There is no such hatch pattern.",
            Self::BadScale => "The pattern scale must be greater than zero.",
            Self::TooDense => "The pattern is too dense for this region at this scale.",
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

    /// The same region filled another way.
    #[must_use]
    pub fn styled(&self, style: HatchStyle) -> Self {
        Self {
            loops: self.loops.clone(),
            style,
        }
    }

    /// Whether the hatch bounds a region and its pattern can be drawn.
    pub fn check(&self) -> Result<(), HatchError> {
        self.check_region()?;
        if let HatchStyle::Pattern {
            name,
            angle,
            scale,
            origin,
        } = &self.style
        {
            if hatch_pattern(name).is_none() {
                return Err(HatchError::UnknownPattern);
            }
            if !(scale.is_finite() && *scale > 0.0 && angle.is_finite() && origin.is_finite()) {
                return Err(HatchError::BadScale);
            }
            if self.lines().is_none() {
                return Err(HatchError::TooDense);
            }
        }
        Ok(())
    }

    /// The pattern's families, placed; empty for a solid fill or a name
    /// the table lacks.
    pub fn families(&self) -> Vec<LineFamily> {
        let HatchStyle::Pattern {
            name,
            angle,
            scale,
            origin,
        } = &self.style
        else {
            return Vec::new();
        };
        hatch_pattern(name).map_or_else(Vec::new, |pattern| {
            pattern
                .families
                .iter()
                .map(|family| family.placed(*angle, *scale, *origin))
                .collect()
        })
    }

    /// The pattern's pieces inside the region — none for a solid fill —
    /// or `None` when there would be more than [`MAX_PATTERN_PIECES`].
    pub fn lines(&self) -> Option<Vec<Segment>> {
        let rings = self.rings();
        let mut out = Vec::new();
        for family in self.families() {
            let budget = MAX_PATTERN_PIECES.saturating_sub(out.len());
            out.extend(pattern_lines(&rings, &family, budget)?);
        }
        Some(out)
    }

    fn check_region(&self) -> Result<(), HatchError> {
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

    /// The triangles that fill the region — what a solid hatch is drawn
    /// with: each outer ring with the holes
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
            // A pattern goes with its hatch: its origin moves, its angle
            // turns, its scale grows. (A mirror reflects the pattern's
            // angle; the families within it keep their own.)
            style: match &self.style {
                HatchStyle::Solid => HatchStyle::Solid,
                HatchStyle::Pattern {
                    name,
                    angle,
                    scale,
                    origin,
                } => HatchStyle::Pattern {
                    name: name.clone(),
                    angle: transform.apply_angle(*angle),
                    scale: scale * transform.scale_factor(),
                    origin: transform.apply(*origin),
                },
            },
        }
    }
}

/// `path`, closed, with a bulge per vertex.
fn closed(mut path: Path) -> Path {
    path.closed = true;
    path.bulges.resize(path.vertices.len(), 0.0);
    path
}

/// How many of its repeats a swatch shows, each way.
const SWATCH_REPEATS: f64 = 4.0;

/// The lines a hatch of `pattern`, at angle 0 and scale 1, draws over a
/// square [`SWATCH_REPEATS`] of its repeats across — its widest spacing or
/// dash period — given in a unit square: how a hatch dialog shows the
/// pattern, by the same clipping a hatch uses (#346). Empty for `SOLID`;
/// `None` for a name the table lacks.
pub fn swatch(pattern: &str) -> Option<Vec<Segment>> {
    let families = &hatch_pattern(pattern)?.families;
    let repeat = families
        .iter()
        .map(|f| {
            let period: f64 = f.dashes.iter().map(|d| d.abs()).sum();
            f.spacing.abs().max(period)
        })
        .fold(0.0, f64::max);
    if repeat <= 0.0 {
        return Some(Vec::new());
    }
    let size = SWATCH_REPEATS * repeat;
    let p = Point2::new;
    let square = Path {
        vertices: vec![p(0.0, 0.0), p(size, 0.0), p(size, size), p(0.0, size)],
        bulges: Vec::new(),
        closed: true,
    };
    let unit = |q: Point2| p(q.x / size, q.y / size);
    let lines = HatchEntity::solid(vec![square])
        .styled(HatchStyle::named(pattern, 0.0, 1.0))
        .lines()?;
    Some(
        lines
            .into_iter()
            .map(|line| Segment {
                start: unit(line.start),
                end: unit(line.end),
            })
            .collect(),
    )
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
        let crosses = |from, to| {
            Entity::Hatch(hatch.clone()).crosses(&Aabb::from_points(from, to), Tables::none())
        };
        assert!(crosses(p(-5.0, 40.0), p(5.0, 60.0)), "the edge");
        assert!(crosses(p(10.0, 10.0), p(20.0, 20.0)), "inside");
        assert!(!crosses(p(45.0, 45.0), p(55.0, 55.0)), "in the hole");
        assert!(!crosses(p(200.0, 0.0), p(210.0, 10.0)));
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

    #[test]
    fn a_pattern_hatch_is_its_lines_clipped_to_the_region() {
        let region = vec![square(0.0, 0.0, 100.0), circle(p(50.0, 50.0), 20.0)];
        let ansi31 =
            HatchEntity::solid(region.clone()).styled(HatchStyle::named("ANSI31", 0.0, 1.0));
        assert!(ansi31.check().is_ok());
        assert_eq!(ansi31.style.name(), "ANSI31");
        let lines = ansi31.lines().unwrap();
        // 45° lines 3.175 apart across the region: their length is its area
        // over the spacing.
        let length: f64 = lines.iter().map(Segment::length).sum();
        let expected = ansi31.area() / 3.175;
        assert!(
            (length - expected).abs() < expected * 0.02,
            "{length} vs {expected}"
        );
        for line in &lines {
            let middle = line.start + (line.end - line.start) * 0.5;
            assert!(ansi31.contains(middle), "{line:?}");
            let direction = line.end - line.start;
            assert!((direction.x - direction.y).abs() < 1e-9 * (1.0 + direction.length()));
        }
        // Twice the scale: half the length; turned 45° more: vertical.
        let scaled = ansi31.styled(HatchStyle::named("ANSI31", 0.0, 2.0));
        let half: f64 = scaled.lines().unwrap().iter().map(Segment::length).sum();
        assert!((half - expected / 2.0).abs() < expected * 0.02);
        let turned = ansi31.styled(HatchStyle::named(
            "ANSI31",
            std::f64::consts::FRAC_PI_4,
            1.0,
        ));
        assert!(
            turned
                .lines()
                .unwrap()
                .iter()
                .all(|line| (line.start.x - line.end.x).abs() < 1e-9)
        );
        // A solid hatch has no lines; SOLID by name is the solid fill.
        assert_eq!(HatchStyle::named("solid", 1.0, 2.0), HatchStyle::Solid);
        assert_eq!(HatchEntity::solid(region.clone()).lines(), Some(Vec::new()));
        // Refusals: a name the table lacks, a scale that is not one, too fine.
        let named = |name: &str, scale: f64| {
            HatchEntity::solid(region.clone())
                .styled(HatchStyle::named(name, 0.0, scale))
                .check()
        };
        assert_eq!(named("NOPE", 1.0), Err(HatchError::UnknownPattern));
        assert_eq!(named("ANSI31", 0.0), Err(HatchError::BadScale));
        assert_eq!(named("ANSI31", f64::NAN), Err(HatchError::BadScale));
        assert_eq!(named("ANSI31", 1e-4), Err(HatchError::TooDense));
    }

    #[test]
    fn a_pattern_goes_with_its_hatch() {
        let hatch = HatchEntity::solid(vec![square(0.0, 0.0, 50.0)])
            .styled(HatchStyle::named("ANSI37", 0.0, 1.0));
        let offset = cad_core::Vec2::new(7.0, 3.0);
        let moved = hatch.transformed(&Transform::translation(offset));
        let (before, after) = (hatch.lines().unwrap(), moved.lines().unwrap());
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert!((a.start + offset).distance_to(b.start) < 1e-9);
        }
        let scaled = hatch.transformed(&Transform::scaling(p(0.0, 0.0), 2.0).unwrap());
        let HatchStyle::Pattern { scale, .. } = scaled.style else {
            panic!("a pattern");
        };
        assert_eq!(scale, 2.0);
        let turned = hatch.transformed(&Transform::rotation(p(0.0, 0.0), 90.0));
        let HatchStyle::Pattern { angle, .. } = turned.style else {
            panic!("a pattern");
        };
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
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

    /// Every pattern of the table has a swatch, each piece inside the unit
    /// square; SOLID's is empty and a name the table lacks has none (#346).
    #[test]
    fn every_pattern_has_a_swatch_inside_its_square() {
        for pattern in crate::hatch_patterns() {
            let lines = swatch(&pattern.name).expect(&pattern.name);
            assert_eq!(lines.is_empty(), pattern.is_solid(), "{}", pattern.name);
            for line in &lines {
                for q in [line.start, line.end] {
                    assert!(
                        (-1e-9..=1.0 + 1e-9).contains(&q.x) && (-1e-9..=1.0 + 1e-9).contains(&q.y),
                        "{}: {q:?}",
                        pattern.name
                    );
                }
            }
        }
        // Four repeats each way: a few lines of the sparsest pattern too.
        assert!(swatch("ANSI31").unwrap().len() >= 4);
        assert!(swatch("ANSI34").unwrap().len() >= 8);
        assert!(swatch("GRASS").unwrap().len() >= 8);
        assert_eq!(swatch("AR-CONC"), None);
    }
}
