//! The selection set. Interaction state: never part of the document or of
//! undo history (DESIGN §19.7).

use std::collections::BTreeSet;

use cad_core::EntityId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    ids: BTreeSet<EntityId>,
}

impl Selection {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.ids.contains(&id)
    }

    /// Selected IDs in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.ids.iter().copied()
    }

    pub fn add(&mut self, id: EntityId) {
        self.ids.insert(id);
    }

    /// Adds `id`; returns true when it was not selected before.
    pub fn insert(&mut self, id: EntityId) -> bool {
        self.ids.insert(id)
    }

    /// Removes `id`; returns true when it was selected.
    pub fn remove(&mut self, id: EntityId) -> bool {
        self.ids.remove(&id)
    }

    /// Makes `id` the only selected entity.
    pub fn replace_with(&mut self, id: EntityId) {
        self.ids.clear();
        self.ids.insert(id);
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// Drops every ID for which `keep` is false.
    pub fn retain(&mut self, keep: impl FnMut(&EntityId) -> bool) {
        self.ids.retain(keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    #[test]
    fn starts_empty() {
        let selection = Selection::default();
        assert!(selection.is_empty());
        assert_eq!(selection.len(), 0);
    }

    #[test]
    fn add_is_idempotent_and_iteration_is_ordered() {
        let mut selection = Selection::default();
        selection.add(id(3));
        selection.add(id(1));
        selection.add(id(3));
        assert_eq!(selection.iter().collect::<Vec<_>>(), vec![id(1), id(3)]);
        assert!(selection.contains(id(1)) && !selection.contains(id(2)));
    }

    #[test]
    fn replace_with_leaves_a_single_entity_selected() {
        let mut selection = Selection::default();
        selection.add(id(1));
        selection.add(id(2));
        selection.replace_with(id(9));
        assert_eq!(selection.iter().collect::<Vec<_>>(), vec![id(9)]);
    }

    #[test]
    fn insert_and_remove_report_whether_anything_changed() {
        let mut selection = Selection::default();
        assert!(selection.insert(id(1)));
        assert!(!selection.insert(id(1)));
        assert!(selection.remove(id(1)));
        assert!(!selection.remove(id(1)));
    }

    #[test]
    fn retain_and_clear() {
        let mut selection = Selection::default();
        (1..=4).for_each(|raw| selection.add(id(raw)));
        selection.retain(|id| id.raw() % 2 == 0);
        assert_eq!(selection.len(), 2);
        selection.clear();
        assert!(selection.is_empty());
    }
}
