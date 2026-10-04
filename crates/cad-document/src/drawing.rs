//! The drawing: the authoritative CAD state.

use std::fmt;
use std::sync::Arc;

use cad_core::{Aabb, EntityId};

use crate::history::History;
use crate::id_alloc::EntityIdAllocator;
use crate::spatial::RTreeIndex;
use crate::{
    Change, Entity, EntityChange, EntityStore, Revision, SharedEntity, SpatialIndex, Transaction,
};

/// A 2D drawing.
///
/// Reads are open. Writes go through exactly three doors — [`transaction`],
/// [`undo`], and [`redo`] — each of which keeps the entity store, the spatial
/// index, the history, and the revision consistent.
///
/// [`transaction`]: Drawing::transaction
/// [`undo`]: Drawing::undo
/// [`redo`]: Drawing::redo
#[derive(Debug)]
pub struct Drawing {
    entities: EntityStore,
    ids: EntityIdAllocator,
    index: Box<dyn SpatialIndex>,
    history: History,
    revision: Revision,
}

/// What a commit, undo, or redo did, so interaction state such as the
/// selection can follow along without being stored in history (DESIGN §19.7).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Entities that now exist and did not before.
    pub inserted: Vec<EntityId>,
    /// Entities that no longer exist.
    pub removed: Vec<EntityId>,
    /// Entities whose geometry or properties changed.
    pub replaced: Vec<EntityId>,
}

/// Why a change could not be added to a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// The entity does not exist in the drawing.
    UnknownEntity(EntityId),
    /// The entity was already changed earlier in this transaction.
    AlreadyChanged(EntityId),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntity(id) => write!(f, "entity {id} does not exist"),
            Self::AlreadyChanged(id) => {
                write!(f, "entity {id} was already changed in this transaction")
            }
        }
    }
}

impl std::error::Error for TransactionError {}

impl Default for Drawing {
    fn default() -> Self {
        Self::new()
    }
}

impl Drawing {
    /// An empty drawing using the default spatial index.
    pub fn new() -> Self {
        Self::with_index(Box::new(RTreeIndex::new()))
    }

    /// An empty drawing using the given spatial index implementation.
    pub fn with_index(index: Box<dyn SpatialIndex>) -> Self {
        Self {
            entities: EntityStore::new(),
            ids: EntityIdAllocator::new(),
            index,
            history: History::default(),
            revision: Revision::next(),
        }
    }

    /// A drawing pre-populated with `entities` and an empty history.
    ///
    /// This is the import path: opening a file starts a fresh drawing rather
    /// than recording an undoable edit.
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut drawing = Self::new();
        for entity in entities {
            let id = drawing.ids.allocate();
            drawing
                .entities
                .insert(id, Arc::new(entity))
                .expect("freshly allocated IDs are unique");
        }
        drawing.index.rebuild(
            &mut drawing
                .entities
                .iter()
                .map(|(id, entity)| (id, entity.bounds())),
        );
        drawing
    }

    /// Changes on every commit, undo, and redo, and at no other time.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id).map(|entity| &**entity)
    }

    /// Every entity, in dense storage order.
    pub fn entities(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities.iter().map(|(id, entity)| (id, &**entity))
    }

    /// Candidate entities whose bounds intersect `bounds`. Callers follow up
    /// with an exact geometric test.
    pub fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        self.index.query(bounds)
    }

    /// The bounding box of all entities, or `None` for an empty drawing.
    pub fn extents(&self) -> Option<Aabb> {
        self.entities()
            .map(|(_, entity)| entity.bounds())
            .reduce(|a, b| a.union(&b))
    }

    /// The raw value of the next entity ID. Monotonic for the lifetime of the
    /// drawing; never decreases, including through undo (DESIGN §10.2).
    pub fn next_entity_id(&self) -> u64 {
        self.ids.peek()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Starts a transaction. Nothing changes until [`TransactionBuilder::commit`];
    /// dropping the builder abandons it.
    pub fn transaction(&mut self) -> TransactionBuilder<'_> {
        TransactionBuilder {
            drawing: self,
            changes: Vec::new(),
        }
    }

    /// Reverts the most recent transaction. Returns `None` when there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<ApplyReport> {
        let inverse = self.history.step_back()?.inverse();
        Some(self.apply(&inverse))
    }

    /// Re-applies the most recently undone transaction. Returns `None` when
    /// there is nothing to redo.
    pub fn redo(&mut self) -> Option<ApplyReport> {
        let transaction = self.history.step_forward()?.clone();
        Some(self.apply(&transaction))
    }

    /// The single place where canonical state changes. Keeps the store and the
    /// derived spatial index in step, then takes a new revision.
    fn apply(&mut self, transaction: &Transaction) -> ApplyReport {
        let mut report = ApplyReport::default();
        for change in transaction.changes() {
            match change {
                Change::Entity(EntityChange::Insert { id, entity }) => {
                    self.index.insert(*id, entity.bounds());
                    self.entities
                        .insert(*id, Arc::clone(entity))
                        .expect("transactions never insert a live ID");
                    report.inserted.push(*id);
                }
                Change::Entity(EntityChange::Remove { id, .. }) => {
                    self.index.remove(*id);
                    self.entities
                        .remove(*id)
                        .expect("transactions only remove live entities");
                    report.removed.push(*id);
                }
                Change::Entity(EntityChange::Replace { id, after, .. }) => {
                    self.index.update(*id, after.bounds());
                    self.entities
                        .replace(*id, Arc::clone(after))
                        .expect("transactions only replace live entities");
                    report.replaced.push(*id);
                }
            }
        }
        self.revision = Revision::next();
        report
    }
}

/// Collects changes for one atomic, undoable edit.
///
/// Each entity may be changed at most once per transaction, which keeps every
/// recorded change valid against the drawing as it was when the transaction
/// began — so commit cannot fail.
#[derive(Debug)]
pub struct TransactionBuilder<'d> {
    drawing: &'d mut Drawing,
    changes: Vec<Change>,
}

impl TransactionBuilder<'_> {
    /// Adds a new entity and returns its ID. The ID is consumed even if the
    /// transaction is abandoned; sparse IDs are fine (DESIGN §10.2).
    pub fn insert(&mut self, entity: impl Into<Entity>) -> EntityId {
        let id = self.drawing.ids.allocate();
        self.push(EntityChange::Insert {
            id,
            entity: Arc::new(entity.into()),
        });
        id
    }

    /// Removes an existing entity.
    pub fn remove(&mut self, id: EntityId) -> Result<(), TransactionError> {
        let entity = self.existing(id)?;
        self.push(EntityChange::Remove { id, entity });
        Ok(())
    }

    /// Replaces an existing entity's content, keeping its ID.
    pub fn replace(
        &mut self,
        id: EntityId,
        entity: impl Into<Entity>,
    ) -> Result<(), TransactionError> {
        let before = self.existing(id)?;
        self.push(EntityChange::Replace {
            id,
            before,
            after: Arc::new(entity.into()),
        });
        Ok(())
    }

    /// Applies the changes as one undo step and discards any redo branch.
    /// An empty transaction is a no-op: no history entry, no new revision.
    pub fn commit(self) -> ApplyReport {
        if self.changes.is_empty() {
            return ApplyReport::default();
        }
        let transaction = Transaction::new(self.changes);
        let report = self.drawing.apply(&transaction);
        self.drawing.history.record(transaction);
        report
    }

    fn push(&mut self, change: EntityChange) {
        self.changes.push(Change::Entity(change));
    }

    fn existing(&self, id: EntityId) -> Result<SharedEntity, TransactionError> {
        let already_changed = self
            .changes
            .iter()
            .any(|Change::Entity(change)| change.id() == id);
        if already_changed {
            return Err(TransactionError::AlreadyChanged(id));
        }
        self.drawing
            .entities
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownEntity(id))
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;
    use crate::LineEntity;
    use crate::spatial::LinearIndex;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
        LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    fn create(drawing: &mut Drawing, entity: LineEntity) -> EntityId {
        let mut tx = drawing.transaction();
        let id = tx.insert(entity);
        tx.commit();
        id
    }

    fn everything() -> Aabb {
        Aabb::from_points(Point2::new(-1e9, -1e9), Point2::new(1e9, 1e9))
    }

    fn sorted(mut ids: Vec<EntityId>) -> Vec<EntityId> {
        ids.sort_unstable();
        ids
    }

    fn live_ids(drawing: &Drawing) -> Vec<EntityId> {
        sorted(drawing.entities().map(|(id, _)| id).collect())
    }

    #[test]
    fn a_new_drawing_is_empty() {
        let drawing = Drawing::new();
        assert!(drawing.is_empty());
        assert_eq!(drawing.extents(), None);
        assert!(!drawing.can_undo());
        assert!(!drawing.can_redo());
    }

    #[test]
    fn committed_entities_are_stored_indexed_and_reported() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let id = tx.insert(line(0.0, 0.0, 10.0, 0.0));
        let report = tx.commit();

        assert_eq!(
            report,
            ApplyReport {
                inserted: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.query(&everything()), vec![id]);
        assert_eq!(drawing.len(), 1);
    }

    #[test]
    fn nothing_changes_until_commit() {
        let mut drawing = Drawing::new();
        let before = drawing.revision();
        let mut tx = drawing.transaction();
        tx.insert(line(0.0, 0.0, 1.0, 1.0));
        drop(tx);

        assert!(drawing.is_empty());
        assert_eq!(drawing.revision(), before);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn an_empty_transaction_is_a_no_op() {
        let mut drawing = Drawing::new();
        let before = drawing.revision();
        assert_eq!(drawing.transaction().commit(), ApplyReport::default());
        assert_eq!(drawing.revision(), before);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn undo_removes_a_created_entity_and_redo_restores_the_same_id() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let undone = drawing.undo().unwrap();
        assert_eq!(
            undone,
            ApplyReport {
                removed: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(drawing.entity(id), None);
        assert_eq!(drawing.query(&everything()), vec![]);

        let redone = drawing.redo().unwrap();
        assert_eq!(
            redone,
            ApplyReport {
                inserted: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.query(&everything()), vec![id]);
    }

    /// DESIGN §10.2: create A, create B, undo B, create C → C gets a new ID.
    #[test]
    fn the_id_allocator_never_rolls_back_through_undo() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        assert_eq!(b.raw(), a.raw() + 1);

        let before_undo = drawing.next_entity_id();
        drawing.undo().unwrap();
        assert_eq!(
            drawing.next_entity_id(),
            before_undo,
            "undo must not lower the counter"
        );

        let c = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        assert_eq!(c.raw(), b.raw() + 1, "B's ID must not be reused for C");
    }

    /// DESIGN §19.3: A, B, C, undo C, undo B, create D → B/C are gone for good.
    #[test]
    fn new_work_after_undo_discards_the_redo_branch_without_reusing_ids() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let c = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert!(drawing.can_redo());

        let d = create(&mut drawing, line(0.0, 3.0, 1.0, 3.0));

        assert!(!drawing.can_redo());
        assert_eq!(drawing.redo(), None);
        assert_eq!(live_ids(&drawing), vec![a, d]);
        assert!(d > c && d != b);
    }

    #[test]
    fn remove_and_replace_are_undoable() {
        let mut drawing = Drawing::new();
        let kept = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let erased = create(&mut drawing, line(5.0, 5.0, 6.0, 5.0));

        let mut tx = drawing.transaction();
        tx.replace(kept, line(0.0, 0.0, 100.0, 0.0)).unwrap();
        tx.remove(erased).unwrap();
        let report = tx.commit();
        assert_eq!(report.replaced, vec![kept]);
        assert_eq!(report.removed, vec![erased]);
        assert_eq!(
            drawing.entity(kept),
            Some(&Entity::Line(line(0.0, 0.0, 100.0, 0.0)))
        );
        assert_eq!(drawing.entity(erased), None);

        // One gesture, one undo step: both changes revert together.
        let undone = drawing.undo().unwrap();
        assert_eq!(undone.inserted, vec![erased]);
        assert_eq!(undone.replaced, vec![kept]);
        assert_eq!(
            drawing.entity(kept),
            Some(&Entity::Line(line(0.0, 0.0, 1.0, 0.0)))
        );
        assert_eq!(
            drawing.entity(erased),
            Some(&Entity::Line(line(5.0, 5.0, 6.0, 5.0)))
        );
    }

    #[test]
    fn the_index_follows_replaced_geometry() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let far = Aabb::from_points(Point2::new(900.0, 900.0), Point2::new(1100.0, 1100.0));
        assert_eq!(drawing.query(&far), vec![]);

        let mut tx = drawing.transaction();
        tx.replace(id, line(1000.0, 1000.0, 1001.0, 1000.0))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.query(&far), vec![id]);

        drawing.undo().unwrap();
        assert_eq!(drawing.query(&far), vec![]);
    }

    #[test]
    fn changing_a_missing_entity_is_rejected() {
        let mut drawing = Drawing::new();
        let ghost = EntityId::from_raw(999);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove(ghost),
            Err(TransactionError::UnknownEntity(ghost))
        );
        assert_eq!(
            tx.replace(ghost, line(0.0, 0.0, 1.0, 1.0)),
            Err(TransactionError::UnknownEntity(ghost))
        );
    }

    #[test]
    fn an_entity_can_change_only_once_per_transaction() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let mut tx = drawing.transaction();
        tx.remove(id).unwrap();
        assert_eq!(tx.remove(id), Err(TransactionError::AlreadyChanged(id)));
        assert_eq!(
            tx.replace(id, line(0.0, 0.0, 2.0, 0.0)),
            Err(TransactionError::AlreadyChanged(id))
        );
    }

    #[test]
    fn revision_changes_on_commit_undo_and_redo_only() {
        let mut drawing = Drawing::new();
        let r0 = drawing.revision();

        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let r1 = drawing.revision();
        assert_ne!(r1, r0);

        // Reads leave the revision alone.
        let _ = (
            drawing.entity(id),
            drawing.query(&everything()),
            drawing.extents(),
            drawing.len(),
        );
        assert_eq!(drawing.revision(), r1);

        drawing.undo().unwrap();
        let r2 = drawing.revision();
        assert_ne!(r2, r1);

        drawing.redo().unwrap();
        assert_ne!(drawing.revision(), r2);

        // Nothing left to redo: no change, no new revision.
        let r3 = drawing.revision();
        assert_eq!(drawing.redo(), None);
        assert_eq!(drawing.revision(), r3);
    }

    #[test]
    fn undo_and_redo_on_an_empty_history_do_nothing() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.undo(), None);
        assert_eq!(drawing.redo(), None);
    }

    #[test]
    fn imported_entities_are_indexed_and_not_undoable() {
        let drawing = Drawing::from_entities([
            Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
            Entity::Line(line(10.0, 10.0, 11.0, 10.0)),
        ]);
        assert_eq!(drawing.len(), 2);
        assert_eq!(drawing.query(&everything()).len(), 2);
        assert!(!drawing.can_undo());
        assert_eq!(
            drawing.extents(),
            Some(Aabb::from_points(
                Point2::new(0.0, 0.0),
                Point2::new(11.0, 10.0)
            ))
        );
    }

    #[test]
    fn extents_cover_every_entity() {
        let mut drawing = Drawing::new();
        create(&mut drawing, line(-5.0, 0.0, 1.0, 0.0));
        create(&mut drawing, line(0.0, -3.0, 0.0, 8.0));
        assert_eq!(
            drawing.extents(),
            Some(Aabb::from_points(
                Point2::new(-5.0, -3.0),
                Point2::new(1.0, 8.0)
            ))
        );
    }

    #[derive(Clone, Debug)]
    enum Op {
        Create(f64, f64, f64, f64),
        Erase(usize),
        Modify(usize, f64, f64),
        Undo,
        Redo,
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -500.0..500.0_f64
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            4 => (coordinate(), coordinate(), coordinate(), coordinate())
                .prop_map(|(a, b, c, d)| Op::Create(a, b, c, d)),
            1 => any::<usize>().prop_map(Op::Erase),
            2 => (any::<usize>(), coordinate(), coordinate()).prop_map(|(i, x, y)| Op::Modify(i, x, y)),
            3 => Just(Op::Undo),
            2 => Just(Op::Redo),
        ]
    }

    proptest! {
        /// After any sequence of edits, undos, and redos:
        ///  - the spatial index equals one rebuilt from scratch (§19.5);
        ///  - no entity ID is ever handed out twice (§10.2);
        ///  - the allocator never decreases.
        #[test]
        fn derived_state_and_identity_survive_any_edit_sequence(
            ops in prop::collection::vec(op(), 0..80),
            queries in prop::collection::vec(
                (coordinate(), coordinate(), 0.0..300.0_f64, 0.0..300.0_f64), 1..6),
        ) {
            let mut drawing = Drawing::new();
            let mut allocated: Vec<EntityId> = Vec::new();
            let mut high_water = drawing.next_entity_id();

            for op in ops {
                let live = live_ids(&drawing);
                match op {
                    Op::Create(x0, y0, x1, y1) => {
                        let id = create(&mut drawing, line(x0, y0, x1, y1));
                        prop_assert!(!allocated.contains(&id), "ID {id} was reused");
                        allocated.push(id);
                    }
                    Op::Erase(pick) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        tx.remove(live[pick % live.len()]).unwrap();
                        tx.commit();
                    }
                    Op::Modify(pick, x, y) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        tx.replace(live[pick % live.len()], line(x, y, x + 10.0, y - 10.0)).unwrap();
                        tx.commit();
                    }
                    Op::Erase(_) | Op::Modify(..) => {}
                    Op::Undo => { drawing.undo(); }
                    Op::Redo => { drawing.redo(); }
                }
                prop_assert!(drawing.next_entity_id() >= high_water);
                high_water = drawing.next_entity_id();
            }

            let mut reference = LinearIndex::new();
            reference.rebuild(&mut drawing.entities().map(|(id, entity)| (id, entity.bounds())));

            prop_assert_eq!(sorted(drawing.query(&everything())), live_ids(&drawing));
            for (x, y, w, h) in queries {
                let query = Aabb::from_points(Point2::new(x, y), Point2::new(x + w, y + h));
                prop_assert_eq!(sorted(drawing.query(&query)), sorted(reference.query(&query)));
            }
        }
    }
}
