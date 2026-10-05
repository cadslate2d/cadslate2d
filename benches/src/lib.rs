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
