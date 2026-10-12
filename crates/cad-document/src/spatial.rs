//! Spatial indexing of entity bounds (DESIGN §20).
//!
//! The index is derived state: it can always be rebuilt from the entity store
//! and is never recorded in undo history (§19.5). Callers depend on the
//! [`SpatialIndex`] trait only; the implementation behind it is PROVISIONAL
//! and chosen by benchmark.

use std::collections::{HashMap, HashSet};
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

/// Entries per node of the packed tree: the fan-out of each level.
const PACKED_FANOUT: usize = 16;

/// A static R-tree packed by sort-tile-recursion (ADR-0007): the entries
/// in a spatially coherent order, and above them levels of bounds, each
/// node the bounds of [`PACKED_FANOUT`] consecutive items of the level
/// below. Built in one pass from sorted entries, never inserted into: what
/// is added later lives in the dynamic tree beside it, and what is removed
/// is marked.
#[derive(Debug, Default)]
struct PackedTree {
    entries: Vec<(EntityId, Aabb)>,
    /// `levels[0]` holds the bounds of each run of `PACKED_FANOUT` entries,
    /// `levels[1]` those of each run of `levels[0]`, and so on up to one
    /// node. Empty for fewer entries than one node.
    levels: Vec<Vec<Aabb>>,
}

impl PackedTree {
    /// Packs `entries`: sorted by the centre's x into vertical slices, each
    /// slice sorted by the centre's y, so that consecutive entries are
    /// close together.
    fn pack(mut entries: Vec<(EntityId, Aabb)>) -> Self {
        let center = |bounds: &Aabb| bounds.center();
        entries.sort_by(|(_, a), (_, b)| center(a).x.total_cmp(&center(b).x));
        let leaves = entries.len().div_ceil(PACKED_FANOUT);
        let slices = (leaves as f64).sqrt().ceil().max(1.0) as usize;
        let slice_len = leaves.div_ceil(slices) * PACKED_FANOUT;
        if slice_len > 0 {
            for slice in entries.chunks_mut(slice_len) {
                slice.sort_by(|(_, a), (_, b)| center(a).y.total_cmp(&center(b).y));
            }
        }
        let mut levels: Vec<Vec<Aabb>> = Vec::new();
        let mut below: Vec<Aabb> = entries.iter().map(|(_, bounds)| *bounds).collect();
        while below.len() > 1 {
            let above: Vec<Aabb> = below
                .chunks(PACKED_FANOUT)
                .map(|chunk| {
                    chunk
                        .iter()
                        .copied()
                        .reduce(|a, b| a.union(&b))
                        .expect("a chunk is never empty")
                })
                .collect();
            levels.push(above.clone());
            below = above;
        }
        Self { entries, levels }
    }

    /// Every packed entry whose bounds meet `bounds`, through `keep`.
    fn query(&self, bounds: &Aabb, keep: &dyn Fn(EntityId) -> bool, out: &mut Vec<EntityId>) {
        if self.entries.is_empty() {
            return;
        }
        match self.levels.len() {
            0 => self.leaf(bounds, 0, keep, out),
            top => self.descend(bounds, top - 1, 0, keep, out),
        }
    }

    fn descend(
        &self,
        bounds: &Aabb,
        level: usize,
        node: usize,
        keep: &dyn Fn(EntityId) -> bool,
        out: &mut Vec<EntityId>,
    ) {
        if !self.levels[level][node].intersects(bounds) {
            return;
        }
        let first = node * PACKED_FANOUT;
        if level == 0 {
            self.leaf(bounds, first, keep, out);
        } else {
            let last = (first + PACKED_FANOUT).min(self.levels[level - 1].len());
            for child in first..last {
                self.descend(bounds, level - 1, child, keep, out);
            }
        }
    }

    /// The entries of one leaf: `PACKED_FANOUT` from `first`.
    fn leaf(
        &self,
        bounds: &Aabb,
        first: usize,
        keep: &dyn Fn(EntityId) -> bool,
        out: &mut Vec<EntityId>,
    ) {
        let last = (first + PACKED_FANOUT).min(self.entries.len());
        for (id, entry) in &self.entries[first..last] {
            if entry.intersects(bounds) && keep(*id) {
                out.push(*id);
            }
        }
    }
}

/// Where an entry lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// In the packed tree, from the last rebuild.
    Packed,
    /// In the dynamic tree, inserted since.
    Dynamic,
}

/// R-tree index: the default implementation. A rebuild packs every entry
/// into a static tree in one pass; what is inserted later goes into an
/// R*-tree beside it, and what is removed from the packed tree is marked.
/// The packed tree is repacked from what is left when half of it is gone.
#[derive(Debug, Default)]
pub struct RTreeIndex {
    packed: PackedTree,
    /// Packed entries removed since the rebuild.
    gone: HashSet<EntityId>,
    tree: RTree<RTreeEntry>,
    // Removal needs the entry's bounds and where it is; remember them per ID.
    bounds: HashMap<EntityId, (Aabb, Place)>,
}

impl RTreeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Repacks the packed tree from its live entries, once the marked ones
    /// outnumber them: a query must not walk removals for ever.
    fn compact_if_due(&mut self) {
        if self.gone.len() <= self.packed.entries.len() / 2 {
            return;
        }
        let live: Vec<(EntityId, Aabb)> = self
            .packed
            .entries
            .iter()
            .filter(|(id, _)| !self.gone.contains(id))
            .copied()
            .collect();
        self.packed = PackedTree::pack(live);
        self.gone.clear();
    }
}

impl SpatialIndex for RTreeIndex {
    fn insert(&mut self, id: EntityId, bounds: Aabb) {
        let previous = self.bounds.insert(id, (bounds, Place::Dynamic));
        debug_assert!(previous.is_none(), "entity {id} is already indexed");
        self.tree.insert(rtree_entry(id, &bounds));
    }

    fn remove(&mut self, id: EntityId) {
        match self.bounds.remove(&id) {
            Some((bounds, Place::Dynamic)) => {
                self.tree.remove(&rtree_entry(id, &bounds));
            }
            Some((_, Place::Packed)) => {
                self.gone.insert(id);
                self.compact_if_due();
            }
            None => {}
        }
    }

    fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        let mut out: Vec<EntityId> = self
            .tree
            .locate_in_envelope_intersecting(envelope(bounds))
            .map(|entry| entry.data)
            .collect();
        self.packed
            .query(bounds, &|id| !self.gone.contains(&id), &mut out);
        out
    }

    fn rebuild(&mut self, entries: &mut dyn Iterator<Item = (EntityId, Aabb)>) {
        // Not `RTree::bulk_load`: in rstar 0.13.0 a bulk-loaded tree can
        // panic on a later insert (ADR-0007). The packed tree is ours, and
        // is never inserted into.
        let entries: Vec<(EntityId, Aabb)> = entries.collect();
        self.bounds = entries
            .iter()
            .map(|(id, bounds)| (*id, (*bounds, Place::Packed)))
            .collect();
        debug_assert_eq!(
            self.bounds.len(),
            entries.len(),
            "an entity is indexed twice"
        );
        self.packed = PackedTree::pack(entries);
        self.gone.clear();
        self.tree = RTree::new();
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
    fn a_packed_tree_answers_every_query_at_every_size() {
        for size in [0usize, 1, 15, 16, 17, 255, 256, 257, 5000] {
            let entries: Vec<(EntityId, Aabb)> = (0..size)
                .map(|i| {
                    let x = (i * 37 % 101) as f64;
                    let y = (i * 53 % 97) as f64;
                    (id(i as u64 + 1), aabb(x, y, x + 2.0, y + 2.0))
                })
                .collect();
            let packed = PackedTree::pack(entries.clone());
            assert_eq!(packed.entries.len(), size);
            let mut found = Vec::new();
            packed.query(&aabb(10.0, 10.0, 30.0, 30.0), &|_| true, &mut found);
            let expected: Vec<EntityId> = entries
                .iter()
                .filter(|(_, b)| b.intersects(&aabb(10.0, 10.0, 30.0, 30.0)))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(sorted(found), sorted(expected), "{size} entries");
        }
    }

    #[test]
    fn removing_most_of_a_packed_tree_repacks_it() {
        let mut index = RTreeIndex::new();
        let entries: Vec<(EntityId, Aabb)> = (0..100)
            .map(|i| (id(i + 1), aabb(i as f64, 0.0, i as f64 + 0.5, 1.0)))
            .collect();
        index.rebuild(&mut entries.iter().copied());
        for i in 0..60 {
            index.remove(id(i + 1));
        }
        // Repacked at the 51st removal to the 49 then live; nine more since.
        assert_eq!((index.packed.entries.len(), index.gone.len()), (49, 9));
        assert_eq!(index.len(), 40);
        assert_eq!(index.query(&aabb(-1.0, -1.0, 200.0, 2.0)).len(), 40);
        assert_eq!(index.query(&aabb(59.0, 0.0, 60.9, 1.0)), vec![id(61)]);
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
        /// Liskov check: rebuilt and then edited any way, the R-tree answers
        /// every query exactly like the brute-force reference, and like an
        /// index rebuilt from scratch.
        #[test]
        fn rtree_matches_reference_after_any_edit_sequence(
            packed in prop::collection::vec(any_aabb(), 0..80),
            ops in prop::collection::vec(op(), 0..60),
            queries in prop::collection::vec(any_aabb(), 1..8),
        ) {
            // The tree starts rebuilt from `packed` — as an opened drawing's
            // does — and is then edited: removals and updates of packed
            // entries, inserts beside them, and the repacking when most of
            // the packed entries are gone, all against the reference.
            let mut rtree = RTreeIndex::new();
            let mut reference = LinearIndex::new();
            let mut live: Vec<(EntityId, Aabb)> = packed
                .iter()
                .enumerate()
                .map(|(i, bounds)| (id(i as u64 + 1), *bounds))
                .collect();
            rtree.rebuild(&mut live.iter().copied());
            reference.rebuild(&mut live.iter().copied());
            let mut next = live.len() as u64 + 1;

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
