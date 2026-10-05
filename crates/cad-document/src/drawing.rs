//! The drawing: the authoritative CAD state.

use std::fmt;
use std::sync::Arc;

use cad_core::{Aabb, EntityId};

use crate::history::History;
use crate::id_alloc::EntityIdAllocator;
use crate::layer::{Layer, LayerId, LayerTable, initial_layers};
use crate::spatial::RTreeIndex;
use crate::table::TableChange;
use crate::{
    Change, Entity, EntityChange, EntityRecord, EntityStore, Properties, Revision, SpatialIndex,
    Transaction,
};

/// A 2D drawing.
///
/// Reads are open. Writes go through exactly three doors — [`transaction`],
/// [`undo`], and [`redo`] — each of which keeps the entity store, the tables,
/// the spatial index, the history, and the revision consistent.
///
/// [`transaction`]: Drawing::transaction
/// [`undo`]: Drawing::undo
/// [`redo`]: Drawing::redo
#[derive(Debug)]
pub struct Drawing {
    entities: EntityStore,
    layers: LayerTable,
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
    /// Whether a row of any table changed: a layer made, renamed, removed.
    pub tables_changed: bool,
}

/// Why a change could not be added to a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// The entity does not exist in the drawing.
    UnknownEntity(EntityId),
    /// The entity was already changed earlier in this transaction.
    AlreadyChanged(EntityId),
    /// The layer does not exist, in the drawing or earlier in this transaction.
    UnknownLayer(LayerId),
    /// The layer was already changed earlier in this transaction.
    LayerAlreadyChanged(LayerId),
    /// Layer `0` is always there: it cannot be removed.
    PermanentLayer,
    /// An entity is on the layer, so it cannot be removed.
    LayerInUse(LayerId),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntity(id) => write!(f, "entity {id} does not exist"),
            Self::AlreadyChanged(id) => {
                write!(f, "entity {id} was already changed in this transaction")
            }
            Self::UnknownLayer(id) => write!(f, "layer {id} does not exist"),
            Self::LayerAlreadyChanged(id) => {
                write!(f, "layer {id} was already changed in this transaction")
            }
            Self::PermanentLayer => write!(f, "layer 0 cannot be removed"),
            Self::LayerInUse(id) => write!(f, "layer {id} has entities on it"),
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
            layers: initial_layers(),
            ids: EntityIdAllocator::new(),
            index,
            history: History::default(),
            revision: Revision::next(),
        }
    }

    /// A drawing pre-populated with `entities` — on layer `0`, with default
    /// properties — and an empty history.
    ///
    /// This is the import path: opening a file starts a fresh drawing rather
    /// than recording an undoable edit.
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let entities = entities
            .into_iter()
            .map(|entity| (entity, Properties::default()));
        Self::from_parts([], entities).expect("layer 0 always exists")
    }

    /// A drawing pre-populated with layers and with entities that have
    /// properties, and an empty history.
    ///
    /// `layers` come after layer `0`, which is always there, and take the
    /// IDs 1, 2, … in the order given. An entity on a layer that is not
    /// among them is refused.
    pub fn from_parts(
        layers: impl IntoIterator<Item = Layer>,
        entities: impl IntoIterator<Item = (Entity, Properties)>,
    ) -> Result<Self, TransactionError> {
        let mut drawing = Self::new();
        for layer in layers {
            let id = drawing.layers.allocate();
            drawing
                .layers
                .apply(&TableChange::Insert { id, row: layer });
        }
        for (entity, properties) in entities {
            if !drawing.layers.contains(properties.layer) {
                return Err(TransactionError::UnknownLayer(properties.layer));
            }
            let id = drawing.ids.allocate();
            drawing
                .entities
                .insert(id, EntityRecord::new(entity, properties))
                .expect("freshly allocated IDs are unique");
        }
        drawing.index.rebuild(
            &mut drawing
                .entities
                .iter()
                .map(|(id, entity)| (id, entity.bounds())),
        );
        Ok(drawing)
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

    /// The layer, colour, linetype, and lineweight of an entity.
    pub fn properties(&self, id: EntityId) -> Option<Properties> {
        self.entities.record(id).map(|record| record.properties)
    }

    /// Every entity with its properties, in dense storage order.
    pub fn records(&self) -> impl Iterator<Item = (EntityId, &EntityRecord)> {
        self.entities.records()
    }

    /// The layer table. It always holds layer `0`.
    pub fn layers(&self) -> &LayerTable {
        &self.layers
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

    /// Starts an undo group. Transactions committed until [`Drawing::end_group`]
    /// are undone and redone together, as one step. Commands use this so that
    /// one invocation is one undo step however many transactions it commits.
    pub fn begin_group(&mut self) {
        self.history.begin_group();
    }

    /// Ends the open undo group. A no-op when none is open.
    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the latest transaction committed inside the open undo group
    /// and forgets it: unlike [`Drawing::undo`] it cannot be redone. This is
    /// the "Undo" option *inside* a command. Returns `None` when the group
    /// holds nothing to roll back.
    pub fn rollback_last_in_group(&mut self) -> Option<ApplyReport> {
        let inverse = self.history.take_last_in_group()?.inverse();
        Some(self.apply(&inverse))
    }

    /// The single place where canonical state changes. Keeps the store, the
    /// tables, and the derived spatial index in step, then takes a new
    /// revision.
    fn apply(&mut self, transaction: &Transaction) -> ApplyReport {
        let mut report = ApplyReport::default();
        for change in transaction.changes() {
            match change {
                Change::Entity(EntityChange::Insert { id, record }) => {
                    self.index.insert(*id, record.entity.bounds());
                    self.entities
                        .insert(*id, record.clone())
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
                Change::Entity(EntityChange::Replace { id, before, after }) => {
                    // A change of properties alone leaves the index as it is.
                    if !Arc::ptr_eq(&before.entity, &after.entity) {
                        self.index.update(*id, after.entity.bounds());
                    }
                    self.entities
                        .replace(*id, after.clone())
                        .expect("transactions only replace live entities");
                    report.replaced.push(*id);
                }
                Change::Layer(change) => {
                    self.layers.apply(change);
                    report.tables_changed = true;
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
    ///
    /// The entity gets the properties new objects are given: layer `0`,
    /// everything by layer. An object made *from* another — a copy, an
    /// offset, the second piece of a break — takes that object's properties
    /// with [`insert_like`](Self::insert_like) instead.
    pub fn insert(&mut self, entity: impl Into<Entity>) -> EntityId {
        self.insert_record(EntityRecord::new(entity, Properties::default()))
    }

    /// Adds a new entity with the given properties.
    pub fn insert_with(
        &mut self,
        entity: impl Into<Entity>,
        properties: Properties,
    ) -> Result<EntityId, TransactionError> {
        self.known_layer(properties.layer)?;
        Ok(self.insert_record(EntityRecord::new(entity, properties)))
    }

    /// Adds a new entity with the properties of `source`, an entity of the
    /// drawing as it was when the transaction began.
    pub fn insert_like(
        &mut self,
        source: EntityId,
        entity: impl Into<Entity>,
    ) -> Result<EntityId, TransactionError> {
        let properties = self
            .drawing
            .properties(source)
            .ok_or(TransactionError::UnknownEntity(source))?;
        Ok(self.insert_record(EntityRecord::new(entity, properties)))
    }

    /// Removes an existing entity.
    pub fn remove(&mut self, id: EntityId) -> Result<(), TransactionError> {
        let record = self.existing(id)?;
        self.push(EntityChange::Remove { id, record });
        Ok(())
    }

    /// Replaces an existing entity's geometry, keeping its ID and its
    /// properties.
    pub fn replace(
        &mut self,
        id: EntityId,
        entity: impl Into<Entity>,
    ) -> Result<(), TransactionError> {
        let before = self.existing(id)?;
        let after = before.with_entity(entity);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Changes an existing entity's properties, keeping its ID and sharing
    /// its geometry.
    pub fn set_properties(
        &mut self,
        id: EntityId,
        properties: Properties,
    ) -> Result<(), TransactionError> {
        self.known_layer(properties.layer)?;
        let before = self.existing(id)?;
        let after = before.with_properties(properties);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Adds a layer and returns its ID. Like an entity's, the ID is
    /// consumed even if the transaction is abandoned.
    pub fn add_layer(&mut self, layer: Layer) -> LayerId {
        let id = self.drawing.layers.allocate();
        self.changes
            .push(Change::Layer(TableChange::Insert { id, row: layer }));
        id
    }

    /// Replaces an existing layer's row — its name — keeping its ID, and so
    /// every entity on it.
    pub fn replace_layer(&mut self, id: LayerId, layer: Layer) -> Result<(), TransactionError> {
        let before = self.existing_layer(id)?;
        self.changes.push(Change::Layer(TableChange::Replace {
            id,
            before,
            after: layer,
        }));
        Ok(())
    }

    /// Removes a layer that has no entities on it. Layer `0` stays.
    pub fn remove_layer(&mut self, id: LayerId) -> Result<(), TransactionError> {
        if id == LayerId::FIRST {
            return Err(TransactionError::PermanentLayer);
        }
        let row = self.existing_layer(id)?;
        // In use by an entity of the drawing, or by one this transaction
        // puts on it. (An entity this transaction takes *off* it still
        // counts: remove the layer in a transaction of its own.)
        let on_layer = |record: &EntityRecord| record.properties.layer == id;
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => on_layer(record),
            Change::Entity(EntityChange::Replace { after, .. }) => on_layer(after),
            Change::Entity(EntityChange::Remove { .. }) | Change::Layer(_) => false,
        });
        if pending || self.drawing.records().any(|(_, record)| on_layer(record)) {
            return Err(TransactionError::LayerInUse(id));
        }
        self.changes
            .push(Change::Layer(TableChange::Remove { id, row }));
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

    fn insert_record(&mut self, record: EntityRecord) -> EntityId {
        let id = self.drawing.ids.allocate();
        self.push(EntityChange::Insert { id, record });
        id
    }

    fn push(&mut self, change: EntityChange) {
        self.changes.push(Change::Entity(change));
    }

    fn existing(&self, id: EntityId) -> Result<EntityRecord, TransactionError> {
        let already_changed = self.changes.iter().any(|change| match change {
            Change::Entity(change) => change.id() == id,
            Change::Layer(_) => false,
        });
        if already_changed {
            return Err(TransactionError::AlreadyChanged(id));
        }
        self.drawing
            .entities
            .record(id)
            .cloned()
            .ok_or(TransactionError::UnknownEntity(id))
    }

    /// How this transaction has changed the layer `id` so far, if it has.
    fn layer_change(&self, id: LayerId) -> Option<&TableChange<Layer>> {
        self.changes.iter().find_map(|change| match change {
            Change::Layer(change) if change.id() == id => Some(change),
            _ => None,
        })
    }

    /// A layer of the drawing that this transaction has not touched.
    fn existing_layer(&self, id: LayerId) -> Result<Layer, TransactionError> {
        if self.layer_change(id).is_some() {
            return Err(TransactionError::LayerAlreadyChanged(id));
        }
        self.drawing
            .layers
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayer(id))
    }

    /// Whether an entity may be put on layer `id`: it exists and this
    /// transaction does not remove it, or this transaction adds it.
    fn known_layer(&self, id: LayerId) -> Result<(), TransactionError> {
        let known = match self.layer_change(id) {
            Some(TableChange::Remove { .. }) => false,
            Some(TableChange::Insert { .. } | TableChange::Replace { .. }) => true,
            None => self.drawing.layers.contains(id),
        };
        if known {
            Ok(())
        } else {
            Err(TransactionError::UnknownLayer(id))
        }
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

    #[test]
    fn a_group_is_undone_and_redone_as_one_step() {
        let mut drawing = Drawing::new();
        let before = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));

        drawing.begin_group();
        let a = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let b = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        let mut tx = drawing.transaction();
        tx.replace(a, line(0.0, 1.0, 9.0, 1.0)).unwrap();
        tx.commit();
        drawing.end_group();

        let undone = drawing.undo().unwrap();
        assert_eq!(live_ids(&drawing), vec![before]);
        assert_eq!(sorted(undone.removed), vec![a, b]);

        drawing.redo().unwrap();
        assert_eq!(live_ids(&drawing), vec![before, a, b]);
        assert_eq!(
            drawing.entity(a),
            Some(&Entity::Line(line(0.0, 1.0, 9.0, 1.0)))
        );

        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert!(drawing.is_empty() && !drawing.can_undo());
    }

    #[test]
    fn a_group_with_one_or_no_transactions_changes_nothing() {
        let mut drawing = Drawing::new();
        drawing.begin_group();
        drawing.end_group();
        assert!(!drawing.can_undo());

        drawing.begin_group();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        drawing.end_group();
        drawing.end_group(); // closing twice is harmless
        drawing.undo().unwrap();
        assert_eq!(drawing.entity(id), None);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn rolling_back_inside_a_group_is_not_redoable() {
        let mut drawing = Drawing::new();
        let outside = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        assert_eq!(drawing.rollback_last_in_group(), None, "no group is open");

        drawing.begin_group();
        assert_eq!(drawing.rollback_last_in_group(), None, "the group is empty");
        let kept = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let dropped = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));

        let report = drawing.rollback_last_in_group().unwrap();
        assert_eq!(report.removed, vec![dropped]);
        assert!(
            !drawing.can_redo(),
            "a rolled-back step is forgotten, not redoable"
        );
        drawing.end_group();

        assert_eq!(live_ids(&drawing), vec![outside, kept]);
        // Rolling back never reaches transactions from before the group.
        drawing.begin_group();
        assert_eq!(drawing.rollback_last_in_group(), None);
        assert_eq!(live_ids(&drawing), vec![outside, kept]);
    }

    #[test]
    fn undo_and_redo_inside_a_group_do_not_confuse_it() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));

        // What the UNDO command does: it runs inside its own group.
        drawing.begin_group();
        drawing.undo().unwrap();
        drawing.end_group();
        assert_eq!(live_ids(&drawing), vec![a]);

        drawing.begin_group();
        drawing.redo().unwrap();
        drawing.end_group();
        assert_eq!(live_ids(&drawing), vec![a, b]);

        // Still two separate steps.
        drawing.undo().unwrap();
        assert_eq!(live_ids(&drawing), vec![a]);
    }

    /// DESIGN §19.4: editing a very large entity must not produce copies of it.
    /// History shares the payload, so undo puts back the very same allocation.
    #[test]
    fn undoing_an_edit_to_a_large_polyline_restores_the_original_payload_without_copying() {
        use crate::PolylineEntity;
        let vertices: Vec<Point2> = (0..10_000)
            .map(|i| Point2::new(f64::from(i), f64::from(i % 7)))
            .collect();
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let id = tx.insert(PolylineEntity::new(vertices.clone(), false));
        tx.commit();
        let original: *const Entity = drawing.entity(id).unwrap();

        let mut moved = vertices;
        moved[5000] = Point2::new(-1.0, -1.0);
        let mut tx = drawing.transaction();
        tx.replace(id, PolylineEntity::new(moved, false)).unwrap();
        tx.commit();
        let edited: *const Entity = drawing.entity(id).unwrap();
        assert!(!std::ptr::eq(original, edited));

        drawing.undo().unwrap();
        assert!(
            std::ptr::eq(original, drawing.entity(id).unwrap()),
            "undo must reuse the stored payload"
        );
        drawing.redo().unwrap();
        assert!(
            std::ptr::eq(edited, drawing.entity(id).unwrap()),
            "redo must reuse the stored payload"
        );
    }

    // ── properties and layers (#131, ADR-0013) ──

    fn add_layer(drawing: &mut Drawing, name: &str) -> LayerId {
        let mut tx = drawing.transaction();
        let id = tx.add_layer(Layer::named(name));
        tx.commit();
        id
    }

    fn red_on(layer: LayerId) -> Properties {
        Properties {
            color: crate::Color::Index(1),
            ..Properties::on(layer)
        }
    }

    #[test]
    fn a_new_drawing_has_layer_zero_and_new_entities_are_on_it() {
        let mut drawing = Drawing::new();
        let layers: Vec<(LayerId, &str)> = drawing
            .layers()
            .iter()
            .map(|(id, layer)| (id, layer.name.as_str()))
            .collect();
        assert_eq!(layers, vec![(LayerId::FIRST, "0")]);

        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));
        assert_eq!(drawing.properties(id), Some(Properties::default()));
        assert_eq!(drawing.properties(EntityId::from_raw(999)), None);
        let imported = Drawing::from_entities([Entity::Line(line(0.0, 0.0, 1.0, 1.0))]);
        assert!(
            imported
                .records()
                .all(|(_, record)| record.properties == Properties::default())
        );
    }

    #[test]
    fn a_drawing_can_start_with_layers_and_styled_entities_and_no_history() {
        let walls = LayerId::from_raw(1);
        let drawing = Drawing::from_parts(
            [Layer::named("walls")],
            [
                (Entity::Line(line(0.0, 0.0, 10.0, 0.0)), red_on(walls)),
                (
                    Entity::Line(line(0.0, 5.0, 10.0, 5.0)),
                    Properties::default(),
                ),
            ],
        )
        .unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
        let styles: Vec<Properties> = live_ids(&drawing)
            .into_iter()
            .filter_map(|id| drawing.properties(id))
            .collect();
        assert_eq!(styles, vec![red_on(walls), Properties::default()]);
        assert!(!drawing.can_undo());
        assert_eq!(drawing.query(&everything()).len(), 2, "indexed");

        let nowhere = LayerId::from_raw(7);
        let refused = Drawing::from_parts(
            [],
            [(
                Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
                Properties::on(nowhere),
            )],
        );
        assert_eq!(refused.err(), Some(TransactionError::UnknownLayer(nowhere)));
    }

    #[test]
    fn replacing_geometry_keeps_properties_and_restyling_keeps_geometry() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), red_on(walls))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id), Some(red_on(walls)));

        let mut tx = drawing.transaction();
        tx.replace(id, line(5.0, 5.0, 9.0, 9.0)).unwrap();
        tx.commit();
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(5.0, 5.0, 9.0, 9.0)))
        );
        assert_eq!(
            drawing.properties(id),
            Some(red_on(walls)),
            "kept through the edit"
        );

        let geometry: *const Entity = drawing.entity(id).unwrap();
        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::default()).unwrap();
        let report = tx.commit();
        assert_eq!(report.replaced, vec![id]);
        assert_eq!(drawing.properties(id), Some(Properties::default()));
        assert!(
            std::ptr::eq(geometry, drawing.entity(id).unwrap()),
            "a change of properties shares the geometry"
        );

        drawing.undo().unwrap();
        assert_eq!(drawing.properties(id), Some(red_on(walls)));
        drawing.undo().unwrap();
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.properties(id), Some(red_on(walls)));
    }

    #[test]
    fn an_entity_made_from_another_takes_its_properties() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let mut tx = drawing.transaction();
        let source = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), red_on(walls))
            .unwrap();
        tx.commit();

        let mut tx = drawing.transaction();
        let copy = tx.insert_like(source, line(0.0, 5.0, 10.0, 5.0)).unwrap();
        assert_eq!(
            tx.insert_like(EntityId::from_raw(999), line(0.0, 0.0, 1.0, 0.0)),
            Err(TransactionError::UnknownEntity(EntityId::from_raw(999)))
        );
        tx.commit();
        assert_eq!(drawing.properties(copy), Some(red_on(walls)));

        // An erased entity comes back with the properties it had.
        let mut tx = drawing.transaction();
        tx.remove(copy).unwrap();
        tx.commit();
        drawing.undo().unwrap();
        assert_eq!(drawing.properties(copy), Some(red_on(walls)));
    }

    #[test]
    fn layers_are_added_renamed_and_removed_in_the_same_history_as_entities() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        assert_eq!(walls.raw(), 1);
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(walls))
            .unwrap();
        tx.commit();

        // A rename changes one row; the entity still refers to the same layer.
        let mut tx = drawing.transaction();
        tx.replace_layer(walls, Layer::named("partitions")).unwrap();
        let report = tx.commit();
        assert!(report.tables_changed);
        assert!(report.replaced.is_empty(), "no entity was touched");
        assert_eq!(
            drawing.layers().get(walls),
            Some(&Layer::named("partitions"))
        );
        assert_eq!(drawing.properties(id).unwrap().layer, walls);

        // Undo walks back through layer and entity changes alike.
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
        drawing.undo().unwrap();
        assert_eq!(drawing.entity(id), None);
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().len(), 1);
        assert!(!drawing.can_undo());

        drawing.redo().unwrap();
        drawing.redo().unwrap();
        drawing.redo().unwrap();
        assert_eq!(
            drawing.layers().get(walls),
            Some(&Layer::named("partitions"))
        );
        assert_eq!(drawing.properties(id).unwrap().layer, walls);

        // A layer made after an undone one never takes its ID.
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(add_layer(&mut drawing, "doors").raw(), 2);
    }

    #[test]
    fn a_layer_and_what_is_on_it_can_be_made_in_one_transaction() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let layer = tx.add_layer(Layer::named("walls"));
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(layer))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id).unwrap().layer, layer);
        drawing.undo().unwrap();
        assert_eq!((drawing.len(), drawing.layers().len()), (0, 1), "one step");
        drawing.redo().unwrap();
        assert_eq!(drawing.properties(id).unwrap().layer, layer);
    }

    #[test]
    fn layer_changes_that_would_break_the_drawing_are_refused() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let empty = add_layer(&mut drawing, "empty");
        let nowhere = LayerId::from_raw(99);
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(walls))
            .unwrap();
        tx.commit();
        let revision = drawing.revision();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_layer(LayerId::FIRST),
            Err(TransactionError::PermanentLayer)
        );
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::LayerInUse(walls))
        );
        assert_eq!(
            tx.remove_layer(nowhere),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.replace_layer(nowhere, Layer::named("x")),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.insert_with(line(0.0, 0.0, 1.0, 0.0), Properties::on(nowhere)),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.set_properties(id, Properties::on(nowhere)),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        // Nothing was recorded by any refusal.
        assert_eq!(tx.commit(), ApplyReport::default());
        assert_eq!(drawing.revision(), revision);

        // Within one transaction: a layer is changed once, a removed layer
        // takes no entities, and a layer an entity was just put on stays.
        let mut tx = drawing.transaction();
        tx.replace_layer(empty, Layer::named("spare")).unwrap();
        assert_eq!(
            tx.replace_layer(empty, Layer::named("again")),
            Err(TransactionError::LayerAlreadyChanged(empty))
        );
        assert_eq!(
            tx.remove_layer(empty),
            Err(TransactionError::LayerAlreadyChanged(empty))
        );
        drop(tx);

        let mut tx = drawing.transaction();
        tx.remove_layer(empty).unwrap();
        assert_eq!(
            tx.set_properties(id, Properties::on(empty)),
            Err(TransactionError::UnknownLayer(empty))
        );
        drop(tx);

        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::on(empty)).unwrap();
        assert_eq!(
            tx.remove_layer(empty),
            Err(TransactionError::LayerInUse(empty))
        );
        tx.commit();
        // Now `walls` is free and can go.
        let mut tx = drawing.transaction();
        tx.remove_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.layers().get(walls), None);
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
    }

    #[derive(Clone, Debug)]
    enum Op {
        Create(f64, f64, f64, f64, usize),
        Erase(usize),
        Modify(usize, f64, f64),
        Restyle(usize, usize, u8),
        AddLayer(u8),
        RenameLayer(usize, u8),
        RemoveLayer(usize),
        /// A layer, an entity on it, and an edit of another, as one step.
        Together(usize, f64),
        Undo,
        Redo,
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -500.0..500.0_f64
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            4 => (coordinate(), coordinate(), coordinate(), coordinate(), any::<usize>())
                .prop_map(|(a, b, c, d, layer)| Op::Create(a, b, c, d, layer)),
            1 => any::<usize>().prop_map(Op::Erase),
            2 => (any::<usize>(), coordinate(), coordinate()).prop_map(|(i, x, y)| Op::Modify(i, x, y)),
            2 => (any::<usize>(), any::<usize>(), 1..8u8).prop_map(|(i, layer, color)| Op::Restyle(i, layer, color)),
            2 => any::<u8>().prop_map(Op::AddLayer),
            1 => (any::<usize>(), any::<u8>()).prop_map(|(i, name)| Op::RenameLayer(i, name)),
            1 => any::<usize>().prop_map(Op::RemoveLayer),
            1 => (any::<usize>(), coordinate()).prop_map(|(i, x)| Op::Together(i, x)),
            3 => Just(Op::Undo),
            2 => Just(Op::Redo),
        ]
    }

    /// Everything that is saved: every entity with its geometry and
    /// properties, and every layer.
    type Saved = (Vec<(EntityId, Entity, Properties)>, Vec<(LayerId, Layer)>);

    fn saved(drawing: &Drawing) -> Saved {
        let mut entities: Vec<(EntityId, Entity, Properties)> = drawing
            .records()
            .map(|(id, record)| (id, (*record.entity).clone(), record.properties))
            .collect();
        entities.sort_by_key(|(id, ..)| *id);
        let layers = drawing
            .layers()
            .iter()
            .map(|(id, layer)| (id, layer.clone()))
            .collect();
        (entities, layers)
    }

    proptest! {
        /// After any sequence of edits to entities and layers, undos, and
        /// redos:
        ///  - the spatial index equals one rebuilt from scratch (§19.5);
        ///  - no entity or layer ID is ever handed out twice (§10.2);
        ///  - the allocator never decreases;
        ///  - every entity is on a layer that exists;
        ///  - undoing everything gives back the empty drawing, and redoing
        ///    everything gives back exactly the drawing there was (§19.2):
        ///    one history for entities and tables.
        #[test]
        fn derived_state_and_identity_survive_any_edit_sequence(
            ops in prop::collection::vec(op(), 0..80),
            queries in prop::collection::vec(
                (coordinate(), coordinate(), 0.0..300.0_f64, 0.0..300.0_f64), 1..6),
        ) {
            let mut drawing = Drawing::new();
            let empty = saved(&drawing);
            let mut allocated: Vec<EntityId> = Vec::new();
            let mut layers_made: Vec<LayerId> = Vec::new();
            let mut high_water = drawing.next_entity_id();

            for op in ops {
                let live = live_ids(&drawing);
                let layers: Vec<LayerId> = drawing.layers().iter().map(|(id, _)| id).collect();
                let layer = |pick: usize| layers[pick % layers.len()];
                match op {
                    Op::Create(x0, y0, x1, y1, on) => {
                        let mut tx = drawing.transaction();
                        let id = tx.insert_with(line(x0, y0, x1, y1), Properties::on(layer(on))).unwrap();
                        tx.commit();
                        prop_assert!(!allocated.contains(&id), "ID {id} was reused");
                        allocated.push(id);
                    }
                    Op::Erase(pick) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        tx.remove(live[pick % live.len()]).unwrap();
                        tx.commit();
                    }
                    Op::Modify(pick, x, y) if !live.is_empty() => {
                        let id = live[pick % live.len()];
                        let before = drawing.properties(id);
                        let mut tx = drawing.transaction();
                        tx.replace(id, line(x, y, x + 10.0, y - 10.0)).unwrap();
                        tx.commit();
                        prop_assert_eq!(drawing.properties(id), before, "an edit keeps properties");
                    }
                    Op::Restyle(pick, on, color) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        let properties = Properties { color: crate::Color::Index(color), ..Properties::on(layer(on)) };
                        tx.set_properties(live[pick % live.len()], properties).unwrap();
                        tx.commit();
                    }
                    Op::AddLayer(name) => {
                        let id = add_layer(&mut drawing, &format!("layer {name}"));
                        prop_assert!(!layers_made.contains(&id), "layer ID {} was reused", id);
                        layers_made.push(id);
                    }
                    Op::RenameLayer(pick, name) => {
                        let mut tx = drawing.transaction();
                        tx.replace_layer(layer(pick), Layer::named(format!("renamed {name}"))).unwrap();
                        tx.commit();
                    }
                    Op::RemoveLayer(pick) => {
                        let id = layer(pick);
                        let in_use = drawing.records().any(|(_, record)| record.properties.layer == id);
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_layer(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != LayerId::FIRST && !in_use);
                    }
                    Op::Together(pick, x) => {
                        let mut tx = drawing.transaction();
                        let new_layer = tx.add_layer(Layer::named("together"));
                        allocated.push(tx.insert_with(line(x, 0.0, x, 5.0), Properties::on(new_layer)).unwrap());
                        if !live.is_empty() {
                            tx.set_properties(live[pick % live.len()], Properties::on(new_layer)).unwrap();
                        }
                        tx.commit();
                        layers_made.push(new_layer);
                    }
                    Op::Erase(_) | Op::Modify(..) | Op::Restyle(..) => {}
                    Op::Undo => { drawing.undo(); }
                    Op::Redo => { drawing.redo(); }
                }
                prop_assert!(drawing.next_entity_id() >= high_water);
                high_water = drawing.next_entity_id();
                for (id, record) in drawing.records() {
                    prop_assert!(
                        drawing.layers().contains(record.properties.layer),
                        "entity {} is on a layer that does not exist", id
                    );
                }
            }

            let mut reference = LinearIndex::new();
            reference.rebuild(&mut drawing.entities().map(|(id, entity)| (id, entity.bounds())));

            prop_assert_eq!(sorted(drawing.query(&everything())), live_ids(&drawing));
            for (x, y, w, h) in queries {
                let query = Aabb::from_points(Point2::new(x, y), Point2::new(x + w, y + h));
                prop_assert_eq!(sorted(drawing.query(&query)), sorted(reference.query(&query)));
            }

            // All the way back, and all the way forward again.
            let now = saved(&drawing);
            let mut steps = 0;
            while drawing.undo().is_some() {
                steps += 1;
            }
            prop_assert_eq!(saved(&drawing), empty);
            for _ in 0..steps {
                prop_assert!(drawing.redo().is_some());
            }
            prop_assert_eq!(saved(&drawing), now);
            prop_assert_eq!(sorted(drawing.query(&everything())), live_ids(&drawing));
        }
    }
}
