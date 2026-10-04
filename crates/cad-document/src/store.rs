//! Physical entity storage (DESIGN §10.3).
//!
//! Logical identity (`EntityId`) is stable; the storage slot is not. Entities
//! live in a dense vector for fast iteration, and a side table maps each ID to
//! its current slot. Slot indices never appear in the public API, so the
//! implementation can be replaced after benchmarking without touching callers.

use std::collections::HashMap;
use std::fmt;

use cad_core::EntityId;

use crate::SharedEntity;

/// Returned when inserting an ID that is already live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateEntityId(pub EntityId);

impl fmt::Display for DuplicateEntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "entity {} is already in the store", self.0)
    }
}

impl std::error::Error for DuplicateEntityId {}

#[derive(Debug)]
struct Slot {
    id: EntityId,
    entity: SharedEntity,
}

/// Dense entity storage addressed by stable ID.
#[derive(Debug, Default)]
pub struct EntityStore {
    slots: Vec<Slot>,
    id_to_slot: HashMap<EntityId, usize>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.id_to_slot.contains_key(&id)
    }

    pub fn get(&self, id: EntityId) -> Option<&SharedEntity> {
        self.id_to_slot
            .get(&id)
            .map(|&slot| &self.slots[slot].entity)
    }

    /// Visits every live entity exactly once, in storage order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &SharedEntity)> {
        self.slots.iter().map(|slot| (slot.id, &slot.entity))
    }

    /// Stores `entity` under `id`. The ID may be new or previously removed.
    pub fn insert(&mut self, id: EntityId, entity: SharedEntity) -> Result<(), DuplicateEntityId> {
        if self.contains(id) {
            return Err(DuplicateEntityId(id));
        }
        self.id_to_slot.insert(id, self.slots.len());
        self.slots.push(Slot { id, entity });
        Ok(())
    }

    /// Removes and returns the entity. The freed slot is reused immediately by
    /// moving the last entity into it, so storage stays dense.
    pub fn remove(&mut self, id: EntityId) -> Option<SharedEntity> {
        let slot = self.id_to_slot.remove(&id)?;
        let removed = self.slots.swap_remove(slot);
        if let Some(moved) = self.slots.get(slot) {
            self.id_to_slot.insert(moved.id, slot);
        }
        Some(removed.entity)
    }

    /// Replaces the payload stored under `id`, returning the previous one.
    pub fn replace(&mut self, id: EntityId, entity: SharedEntity) -> Option<SharedEntity> {
        let slot = *self.id_to_slot.get(&id)?;
        Some(std::mem::replace(&mut self.slots[slot].entity, entity))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cad_core::Point2;

    use super::*;
    use crate::{Entity, LineEntity};

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    fn line(x: f64) -> SharedEntity {
        Arc::new(Entity::Line(LineEntity::new(
            Point2::new(x, 0.0),
            Point2::new(x, 1.0),
        )))
    }

    fn ids(store: &EntityStore) -> Vec<u64> {
        let mut ids: Vec<u64> = store.iter().map(|(id, _)| id.raw()).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn inserted_entities_are_retrievable_by_id() {
        let mut store = EntityStore::new();
        store.insert(id(7), line(7.0)).unwrap();
        assert_eq!(store.get(id(7)), Some(&line(7.0)));
        assert_eq!(store.get(id(8)), None);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn inserting_a_live_id_is_an_error() {
        let mut store = EntityStore::new();
        store.insert(id(1), line(1.0)).unwrap();
        assert_eq!(
            store.insert(id(1), line(2.0)),
            Err(DuplicateEntityId(id(1)))
        );
        assert_eq!(store.get(id(1)), Some(&line(1.0)));
    }

    #[test]
    fn ids_stay_valid_while_other_entities_are_removed_and_slots_reused() {
        let mut store = EntityStore::new();
        for raw in 1..=5 {
            store.insert(id(raw), line(raw as f64)).unwrap();
        }
        // Removing from the front and middle forces surviving entities to move slots.
        assert_eq!(store.remove(id(1)), Some(line(1.0)));
        assert_eq!(store.remove(id(3)), Some(line(3.0)));
        store.insert(id(6), line(6.0)).unwrap();

        for raw in [2, 4, 5, 6] {
            assert_eq!(store.get(id(raw)), Some(&line(raw as f64)), "id {raw}");
        }
        assert_eq!(ids(&store), vec![2, 4, 5, 6]);
    }

    #[test]
    fn a_removed_id_can_be_restored() {
        let mut store = EntityStore::new();
        store.insert(id(1), line(1.0)).unwrap();
        store.insert(id(2), line(2.0)).unwrap();
        let removed = store.remove(id(1)).unwrap();
        store.insert(id(1), removed).unwrap();
        assert_eq!(store.get(id(1)), Some(&line(1.0)));
        assert_eq!(ids(&store), vec![1, 2]);
    }

    #[test]
    fn removing_an_unknown_id_is_a_no_op() {
        let mut store = EntityStore::new();
        store.insert(id(1), line(1.0)).unwrap();
        assert_eq!(store.remove(id(99)), None);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn replace_swaps_the_payload_and_keeps_the_id() {
        let mut store = EntityStore::new();
        store.insert(id(1), line(1.0)).unwrap();
        assert_eq!(store.replace(id(1), line(9.0)), Some(line(1.0)));
        assert_eq!(store.get(id(1)), Some(&line(9.0)));
        assert_eq!(store.replace(id(2), line(0.0)), None);
    }

    #[test]
    fn iteration_is_dense_and_visits_each_entity_once() {
        let mut store = EntityStore::new();
        for raw in 1..=100 {
            store.insert(id(raw), line(raw as f64)).unwrap();
        }
        for raw in (1..=100).filter(|raw| raw % 3 == 0) {
            store.remove(id(raw));
        }
        let expected: Vec<u64> = (1..=100).filter(|raw| raw % 3 != 0).collect();
        assert_eq!(ids(&store), expected);
        assert_eq!(store.iter().count(), store.len());
    }
}
