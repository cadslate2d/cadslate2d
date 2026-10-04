//! Spatial indexing of entity bounds (DESIGN §20).
//!
//! The index is derived state: it can always be rebuilt from the entity store
//! and is never recorded in undo history (§19.5). Callers depend on the
//! [`SpatialIndex`] trait only; the implementation behind it is PROVISIONAL
//! and chosen by benchmark.

use std::collections::HashMap;
use std::fmt::Debug;

use cad_core::{Aabb, EntityId};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{AABB, RTree};

/// A set of `(EntityId, bounds)` entries that answers box queries.
pub trait SpatialIndex: Debug {
    /// Adds an entry. `id` must not already be present.
    fn insert(&mut self, id: EntityId, bounds: Aabb);

    /// Removes an entry. Unknown IDs are ignored.
    fn remove(&mut self, id: EntityId);

    /// Changes the bounds stored for `id`.
    fn update(&mut self, id: EntityId, bounds: Aabb) {
        self.remove(id);
        self.insert(id, bounds);
    }

    /// Every entry whose bounds intersect `bounds`, in unspecified order.
    /// Never omits an intersecting entry.
    fn query(&self, bounds: &Aabb) -> Vec<EntityId>;

    /// Discards all entries and loads `entries` instead.
    fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>);

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

type RTreeEntry = GeomWithData<Rectangle<[f64; 2]>, EntityId>;

fn envelope(bounds: &Aabb) -> AABB<[f64; 2]> {
    AABB::from_corners(
        [bounds.min().x, bounds.min().y],
        [bounds.max().x, bounds.max().y],
    )
}

fn rtree_entry(id: EntityId, bounds: &Aabb) -> RTreeEntry {
    let rectangle = Rectangle::from_corners(
        [bounds.min().x, bounds.min().y],
        [bounds.max().x, bounds.max().y],
    );
    GeomWithData::new(rectangle, id)
}

/// R*-tree index. The default implementation.
#[derive(Debug, Default)]
pub struct RTreeIndex {
    tree: RTree<RTreeEntry>,
    // Removal from the tree needs the entry's bounds; remember them per ID.
    bounds: HashMap<EntityId, Aabb>,
}

impl RTreeIndex {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SpatialIndex for RTreeIndex {
    fn insert(&mut self, id: EntityId, bounds: Aabb) {
        let previous = self.bounds.insert(id, bounds);
        debug_assert!(previous.is_none(), "entity {id} is already indexed");
        self.tree.insert(rtree_entry(id, &bounds));
    }

    fn remove(&mut self, id: EntityId) {
        if let Some(bounds) = self.bounds.remove(&id) {
            self.tree.remove(&rtree_entry(id, &bounds));
        }
    }

    fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        self.tree
            .locate_in_envelope_intersecting(envelope(bounds))
            .map(|entry| entry.data)
            .collect()
    }

    fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>) {
        // Deliberately not `RTree::bulk_load`. In rstar 0.13.0 a bulk-loaded
        // tree can panic ("This is a bug in rstar") on a later insert; it is
        // reproducible with 100,000 entries (ADR-0007). A tree grown by
        // insertion keeps the invariants the insertion algorithm relies on.
        *self = Self::default();
        for (id, bounds) in entries {
            self.insert(id, bounds);
        }
    }

    fn len(&self) -> usize {
        self.bounds.len()
    }
}

/// Brute-force index that tests every entry.
///
/// It is the reference implementation the R-tree is checked against, and a
/// baseline for benchmarks. Not intended for large drawings.
#[derive(Debug, Default)]
pub struct LinearIndex {
    entries: Vec<(EntityId, Aabb)>,
}

impl LinearIndex {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SpatialIndex for LinearIndex {
    fn insert(&mut self, id: EntityId, bounds: Aabb) {
        debug_assert!(
            self.entries.iter().all(|(existing, _)| *existing != id),
            "entity {id} is already indexed"
        );
        self.entries.push((id, bounds));
    }

    fn remove(&mut self, id: EntityId) {
        if let Some(position) = self
            .entries
            .iter()
            .position(|(existing, _)| *existing == id)
        {
            self.entries.swap_remove(position);
        }
    }

    fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        self.entries
            .iter()
            .filter(|(_, entry_bounds)| entry_bounds.intersects(bounds))
            .map(|(id, _)| *id)
            .collect()
    }

    fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>) {
        self.entries = entries.collect();
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    fn aabb(x0: f64, y0: f64, x1: f64, y1: f64) -> Aabb {
        Aabb::from_points(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    fn sorted(mut ids: Vec<EntityId>) -> Vec<EntityId> {
        ids.sort_unstable();
        ids
    }

    /// The contract every implementation must satisfy.
    fn check_contract(mut index: impl SpatialIndex) {
        assert!(index.is_empty());
        index.insert(id(1), aabb(0.0, 0.0, 10.0, 0.0));
        index.insert(id(2), aabb(20.0, 20.0, 30.0, 30.0));
        index.insert(id(3), aabb(5.0, -5.0, 5.0, 5.0));
        assert_eq!(index.len(), 3);

        assert_eq!(
            sorted(index.query(&aabb(4.0, -1.0, 6.0, 1.0))),
            vec![id(1), id(3)]
        );
        assert_eq!(index.query(&aabb(100.0, 100.0, 101.0, 101.0)), vec![]);
        // Touching counts as intersecting.
        assert_eq!(index.query(&aabb(30.0, 30.0, 40.0, 40.0)), vec![id(2)]);

        index.remove(id(1));
        assert_eq!(index.query(&aabb(4.0, -1.0, 6.0, 1.0)), vec![id(3)]);
        index.remove(id(99));
        assert_eq!(index.len(), 2);

        index.update(id(3), aabb(50.0, 50.0, 51.0, 51.0));
        assert_eq!(index.query(&aabb(4.0, -1.0, 6.0, 1.0)), vec![]);
        assert_eq!(index.query(&aabb(49.0, 49.0, 52.0, 52.0)), vec![id(3)]);

        index.rebuild(&mut [(id(7), aabb(0.0, 0.0, 1.0, 1.0))].into_iter());
        assert_eq!(index.len(), 1);
        assert_eq!(index.query(&aabb(-1.0, -1.0, 2.0, 2.0)), vec![id(7)]);
    }

    #[test]
    fn rtree_index_satisfies_the_contract() {
        check_contract(RTreeIndex::new());
    }

    #[test]
    fn linear_index_satisfies_the_contract() {
        check_contract(LinearIndex::new());
    }

    #[derive(Clone, Debug)]
    enum Op {
        Insert(Aabb),
        Remove(usize),
        Update(usize, Aabb),
    }

    fn any_aabb() -> impl Strategy<Value = Aabb> {
        (
            -100.0..100.0_f64,
            -100.0..100.0_f64,
            0.0..40.0_f64,
            0.0..40.0_f64,
        )
            .prop_map(|(x, y, w, h)| aabb(x, y, x + w, y + h))
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => any_aabb().prop_map(Op::Insert),
            1 => any::<usize>().prop_map(Op::Remove),
            1 => (any::<usize>(), any_aabb()).prop_map(|(i, b)| Op::Update(i, b)),
        ]
    }

    proptest! {
        /// Liskov check: after any edit sequence the R-tree answers every
        /// query exactly like the brute-force reference, and like an index
        /// rebuilt from scratch.
        #[test]
        fn rtree_matches_reference_after_any_edit_sequence(
            ops in prop::collection::vec(op(), 0..60),
            queries in prop::collection::vec(any_aabb(), 1..8),
        ) {
            let mut rtree = RTreeIndex::new();
            let mut reference = LinearIndex::new();
            let mut live: Vec<(EntityId, Aabb)> = Vec::new();
            let mut next = 1;

            for op in ops {
                match op {
                    Op::Insert(bounds) => {
                        let id = id(next);
                        next += 1;
                        rtree.insert(id, bounds);
                        reference.insert(id, bounds);
                        live.push((id, bounds));
                    }
                    Op::Remove(pick) if !live.is_empty() => {
                        let (id, _) = live.swap_remove(pick % live.len());
                        rtree.remove(id);
                        reference.remove(id);
                    }
                    Op::Update(pick, bounds) if !live.is_empty() => {
                        let slot = pick % live.len();
                        live[slot].1 = bounds;
                        rtree.update(live[slot].0, bounds);
                        reference.update(live[slot].0, bounds);
                    }
                    Op::Remove(_) | Op::Update(..) => {}
                }
            }

            let mut rebuilt = RTreeIndex::new();
            rebuilt.rebuild(&mut live.iter().copied());

            prop_assert_eq!(rtree.len(), live.len());
            for query in &queries {
                let expected = sorted(reference.query(query));
                prop_assert_eq!(sorted(rtree.query(query)), expected.clone());
                prop_assert_eq!(sorted(rebuilt.query(query)), expected);
            }
        }
    }
}
