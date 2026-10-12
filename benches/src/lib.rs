//! Deterministic benchmark drawings (DESIGN §42).
//!
//! Performance targets are "goals, not promises until benchmarked", and they
//! "must be validated with generated benchmark drawings". This crate is that
//! generator, plus alternative implementations the benchmarks compare the
//! real ones against.

#![cfg_attr(test, allow(clippy::float_cmp))]

pub mod winding_offset;

use std::collections::HashMap;

use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{LineEntity, SharedEntity};

/// The drawing sizes named by the scale targets in DESIGN §42.
pub const SIZES: [usize; 3] = [10_000, 50_000, 100_000];

/// SplitMix64: tiny, fast, and identical on every platform, so a seed always
/// produces the same drawing.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[low, high)`.
    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + self.unit() * (high - low)
    }

    /// Uniform in `0..bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

/// Side length of the square that `count` generated lines are spread over.
/// It grows with the square root of the count, so density — and therefore the
/// number of candidates under a pick box — stays the same at every size.
pub fn extent(count: usize) -> f64 {
    100.0 * (count as f64).sqrt()
}

/// `count` lines resembling ordinary drafting: mostly short segments at any
/// angle, with one in ten long enough to cross many others.
pub fn lines(count: usize, seed: u64) -> Vec<LineEntity> {
    let mut rng = Rng::new(seed);
    let side = extent(count);
    (0..count)
        .map(|_| {
            let start = Point2::new(rng.range(0.0, side), rng.range(0.0, side));
            let length = if rng.unit() < 0.9 {
                rng.range(1.0, 20.0)
            } else {
                rng.range(50.0, 500.0)
            };
            let angle = rng.range(0.0, std::f64::consts::TAU);
            LineEntity::new(start, start + Vec2::from_polar(length, angle))
        })
        .collect()
}

/// A pick-sized query box at a random position in a drawing of `count` lines.
pub fn pick_box(rng: &mut Rng, count: usize) -> Aabb {
    let side = extent(count);
    let center = Point2::new(rng.range(0.0, side), rng.range(0.0, side));
    Aabb::from_points(center, center).expanded(5.0)
}

/// The obvious alternative to the dense `EntityStore`: a hash map and nothing
/// else. Lookups are the same; iteration walks the map's buckets.
#[derive(Debug, Default)]
pub struct HashMapStore {
    entities: HashMap<EntityId, SharedEntity>,
}

impl HashMapStore {
    pub fn insert(&mut self, id: EntityId, entity: SharedEntity) {
        self.entities.insert(id, entity);
    }

    pub fn remove(&mut self, id: EntityId) -> Option<SharedEntity> {
        self.entities.remove(&id)
    }

    pub fn get(&self, id: EntityId) -> Option<&SharedEntity> {
        self.entities.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &SharedEntity)> {
        self.entities.iter().map(|(id, entity)| (*id, entity))
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// How a mixed drawing is made up, per hundred entities: what a drafter's
/// drawing holds, roughly, with every kind the model has (#171).
///
/// | Kind | Per hundred |
/// | --- | --- |
/// | Lines | 44 |
/// | Circles | 8 |
/// | Arcs | 8 |
/// | Polylines of 4–8 vertices, one in three with an arc segment | 10 |
/// | Texts | 8 |
/// | Paragraphs | 2 |
/// | Block references, to one of four blocks of 4–6 lines | 8 |
/// | Dimensions: linear, aligned, radius | 8 |
/// | Hatches: a square with a round hole, every other one ANSI31 | 4 |
pub const MIXED_PER_HUNDRED: [(&str, usize); 9] = [
    ("line", 44),
    ("circle", 8),
    ("arc", 8),
    ("polyline", 10),
    ("text", 8),
    ("paragraph", 2),
    ("block reference", 8),
    ("dimension", 8),
    ("hatch", 4),
];

/// A drawing of `count` entities in the proportions of
/// [`MIXED_PER_HUNDRED`], spread like [`lines`] over a square whose side
/// grows with the square root of the count, on layer 0 in the `Standard`
/// styles. The same seed gives the same drawing on every platform.
pub fn mixed(count: usize, seed: u64) -> cad_document::Drawing {
    use cad_document::{
        ArcEntity, Block, CircleEntity, DimensionEntity, Drawing, Entity, EntityRecord,
        HatchEntity, HatchStyle, MTextEntity, Measure, Path, PolylineEntity, Properties,
        TextEntity,
    };
    use std::f64::consts::TAU;

    let mut rng = Rng::new(seed);
    let side = extent(count);
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();

    // Four blocks of a few lines each, defined once.
    let blocks: Vec<_> = (0..4)
        .map(|which| {
            let sides = 4 + which;
            let radius = 5.0 + which as f64 * 2.0;
            let corners: Vec<Point2> = (0..sides)
                .map(|i| Point2::ORIGIN + Vec2::from_polar(radius, i as f64 / sides as f64 * TAU))
                .collect();
            let lines = (0..sides)
                .map(|i| {
                    EntityRecord::new(
                        LineEntity::new(corners[i], corners[(i + 1) % sides]),
                        Properties::default(),
                    )
                })
                .collect();
            tx.define_block(Block::new(format!("Part{which}"), Point2::ORIGIN, lines))
                .expect("the block names are fresh")
        })
        .collect();

    let total: usize = MIXED_PER_HUNDRED.iter().map(|(_, n)| n).sum();
    debug_assert_eq!(total, 100);
    for index in 0..count {
        let at = Point2::new(rng.range(0.0, side), rng.range(0.0, side));
        let slot = index % 100;
        let mut kind = "";
        let mut from = 0;
        for (name, per_hundred) in MIXED_PER_HUNDRED {
            if slot < from + per_hundred {
                kind = name;
                break;
            }
            from += per_hundred;
        }
        let entity: Entity = match kind {
            "line" => {
                let length = if rng.unit() < 0.9 {
                    rng.range(1.0, 20.0)
                } else {
                    rng.range(50.0, 500.0)
                };
                LineEntity::new(at, at + Vec2::from_polar(length, rng.range(0.0, TAU))).into()
            }
            "circle" => CircleEntity::new(at, rng.range(1.0, 15.0)).into(),
            "arc" => {
                let start = rng.range(0.0, TAU);
                ArcEntity::new(at, rng.range(2.0, 20.0), start, start + rng.range(0.5, 5.0)).into()
            }
            "polyline" => {
                let vertices = 4 + rng.below(5);
                let mut points = Vec::with_capacity(vertices);
                let mut point = at;
                for _ in 0..vertices {
                    point = point + Vec2::from_polar(rng.range(3.0, 15.0), rng.range(0.0, TAU));
                    points.push(point);
                }
                let bulges: Vec<f64> = (0..vertices)
                    .map(|_| {
                        if rng.unit() < 0.33 {
                            rng.range(-0.8, 0.8)
                        } else {
                            0.0
                        }
                    })
                    .collect();
                PolylineEntity::with_bulges(points, bulges, rng.unit() < 0.5).into()
            }
            "text" => TextEntity::new(at, rng.range(2.0, 6.0), 0.0, format!("N{index}")).into(),
            "paragraph" => MTextEntity::new(
                at,
                60.0,
                3.0,
                0.0,
                format!("Note {index}\nsee detail {}", index % 7),
            )
            .into(),
            "block reference" => cad_document::BlockReferenceEntity::new(
                blocks[rng.below(4)],
                at,
                rng.range(0.5, 2.0),
                rng.range(0.0, TAU),
            )
            .into(),
            "dimension" => {
                let b = at + Vec2::new(rng.range(10.0, 60.0), rng.range(-10.0, 10.0));
                let measure = match index % 3 {
                    0 => Measure::Linear {
                        a: at,
                        b,
                        rotation: 0.0,
                    },
                    1 => Measure::Aligned { a: at, b },
                    _ => Measure::Radius {
                        center: at,
                        radius: rng.range(3.0, 12.0),
                    },
                };
                DimensionEntity::new(measure, at + Vec2::new(5.0, 15.0)).into()
            }
            _ => {
                let size = rng.range(10.0, 30.0);
                let square = Path {
                    vertices: vec![
                        at,
                        at + Vec2::new(size, 0.0),
                        at + Vec2::new(size, size),
                        at + Vec2::new(0.0, size),
                    ],
                    bulges: Vec::new(),
                    closed: true,
                };
                let middle = at + Vec2::new(size / 2.0, size / 2.0);
                let hole = Path {
                    vertices: vec![
                        middle - Vec2::new(size / 5.0, 0.0),
                        middle + Vec2::new(size / 5.0, 0.0),
                    ],
                    bulges: vec![1.0, 1.0],
                    closed: true,
                };
                let hatch = HatchEntity::solid(vec![square, hole]);
                if index % 8 < 4 {
                    hatch.into()
                } else {
                    hatch.styled(HatchStyle::named("ANSI31", 0.0, 1.0)).into()
                }
            }
        };
        tx.insert(entity).expect("a generated entity is valid");
    }
    tx.commit();
    drawing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_always_generates_the_same_drawing() {
        assert_eq!(lines(1000, 42), lines(1000, 42));
        assert_ne!(lines(1000, 42), lines(1000, 43));
    }

    #[test]
    fn generated_lines_are_finite_and_never_degenerate() {
        for line in lines(5000, 7) {
            assert!(line.start.is_finite() && line.end.is_finite());
            assert!(line.start.distance_to(line.end) >= 1.0 - 1e-9);
        }
    }

    #[test]
    fn density_is_independent_of_drawing_size() {
        let density = |count: usize| count as f64 / (extent(count) * extent(count));
        assert!((density(10_000) - density(100_000)).abs() < 1e-12);
    }

    #[test]
    fn random_values_stay_in_range() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            let unit = rng.unit();
            assert!((0.0..1.0).contains(&unit));
            assert!(rng.below(7) < 7);
            assert!((-3.0..5.0).contains(&rng.range(-3.0, 5.0)));
        }
    }
}
