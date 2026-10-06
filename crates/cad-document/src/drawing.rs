//! The drawing: the authoritative CAD state.

use std::fmt;
use std::sync::Arc;

use cad_core::{Aabb, EntityId, LengthUnit};

use crate::history::History;
use crate::id_alloc::EntityIdAllocator;
use crate::layer::{Layer, LayerId, LayerTable, initial_layers};
use crate::linetype::{LinetypeDef, LinetypeError, LinetypeId, LinetypeTable, initial_linetypes};
use crate::names::{NameError, check_name, same_name};
use crate::spatial::RTreeIndex;
use crate::table::TableChange;
use crate::{
    Change, Color, DrawingSettings, Entity, EntityChange, EntityRecord, EntityStore, Linetype,
    Lineweight, Properties, Revision, SettingsChange, SpatialIndex, Transaction,
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
    linetypes: LinetypeTable,
    settings: DrawingSettings,
    ids: EntityIdAllocator,
    index: Box<dyn SpatialIndex>,
    history: History,
    revision: Revision,
    shown_revision: Revision,
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
    /// Layer `0` is always there: it cannot be removed or renamed.
    PermanentLayer,
    /// An entity is on the layer, so it cannot be removed.
    LayerInUse(LayerId),
    /// The name cannot be a layer's name.
    BadLayerName(NameError),
    /// This layer already has the name, give or take its case.
    DuplicateLayerName(LayerId),
    /// The layer is the current one, so it cannot be removed or switched off.
    CurrentLayer(LayerId),
    /// The layer is off, so it cannot be made the current one.
    LayerIsOff(LayerId),
    /// The linetype does not exist, in the drawing or earlier in this
    /// transaction.
    UnknownLinetype(LinetypeId),
    /// The linetype was already changed earlier in this transaction.
    LinetypeAlreadyChanged(LinetypeId),
    /// `Continuous` is always there: it cannot be removed.
    PermanentLinetype,
    /// An entity or a layer has the linetype, or new objects are given
    /// it, so it cannot be removed.
    LinetypeInUse(LinetypeId),
    /// The row cannot be a linetype.
    BadLinetype(LinetypeError),
    /// This linetype already has the name, give or take its case.
    DuplicateLinetypeName(LinetypeId),
    /// `LTSCALE` must be a positive number.
    BadLtscale,
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
            Self::PermanentLayer => write!(f, "layer 0 cannot be removed or renamed"),
            Self::LayerInUse(id) => write!(f, "layer {id} has entities on it"),
            Self::BadLayerName(error) => write!(f, "{}", error.explain("layer")),
            Self::DuplicateLayerName(id) => write!(f, "layer {id} already has that name"),
            Self::CurrentLayer(id) => write!(f, "layer {id} is the current layer"),
            Self::LayerIsOff(id) => write!(f, "layer {id} is off"),
            Self::UnknownLinetype(id) => write!(f, "linetype {id} does not exist"),
            Self::LinetypeAlreadyChanged(id) => {
                write!(f, "linetype {id} was already changed in this transaction")
            }
            Self::PermanentLinetype => write!(f, "linetype Continuous cannot be removed"),
            Self::LinetypeInUse(id) => write!(f, "linetype {id} is in use"),
            Self::BadLinetype(error) => write!(f, "{error}"),
            Self::DuplicateLinetypeName(id) => {
                write!(f, "linetype {id} already has that name")
            }
            Self::BadLtscale => write!(f, "LTSCALE must be a positive number"),
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
            linetypes: initial_linetypes(),
            settings: DrawingSettings::default(),
            ids: EntityIdAllocator::new(),
            index,
            history: History::default(),
            revision: Revision::next(),
            shown_revision: Revision::next(),
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
        Self::from_parts([], [], entities).expect("layer 0 always exists")
    }

    /// A drawing pre-populated with layers, linetypes, and entities that
    /// have properties, and an empty history.
    ///
    /// `layers` come after layer `0`, which is always there, and take the
    /// IDs 1, 2, … in the order given; `linetypes` likewise after
    /// `Continuous`. A row with a name that is not allowed or is already
    /// taken, a layer with a linetype that is not among them, and an entity
    /// on a layer or with a linetype that is not among them, are refused.
    pub fn from_parts(
        layers: impl IntoIterator<Item = Layer>,
        linetypes: impl IntoIterator<Item = LinetypeDef>,
        entities: impl IntoIterator<Item = (Entity, Properties)>,
    ) -> Result<Self, TransactionError> {
        let mut drawing = Self::new();
        for linetype in linetypes {
            linetype.check().map_err(TransactionError::BadLinetype)?;
            if let Some(taken) = drawing.linetypes.find(&linetype.name) {
                return Err(TransactionError::DuplicateLinetypeName(taken));
            }
            let id = drawing.linetypes.allocate();
            drawing
                .linetypes
                .apply(&TableChange::Insert { id, row: linetype });
        }
        let known_linetype = |drawing: &Self, linetype: Linetype| match linetype {
            Linetype::Named(id) if !drawing.linetypes.contains(id) => {
                Err(TransactionError::UnknownLinetype(id))
            }
            _ => Ok(()),
        };
        for layer in layers {
            check_name(&layer.name).map_err(TransactionError::BadLayerName)?;
            if let Some(taken) = drawing.layers.find(&layer.name) {
                return Err(TransactionError::DuplicateLayerName(taken));
            }
            known_linetype(&drawing, layer.linetype)?;
            let id = drawing.layers.allocate();
            drawing
                .layers
                .apply(&TableChange::Insert { id, row: layer });
        }
        for (entity, properties) in entities {
            if !drawing.layers.contains(properties.layer) {
                return Err(TransactionError::UnknownLayer(properties.layer));
            }
            known_linetype(&drawing, properties.linetype)?;
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

    /// Changes whenever what [`shown`](Self::shown) yields could have
    /// changed — an entity inserted, removed, or replaced (its properties
    /// count), or a layer switched on or off — and at no other time.
    ///
    /// A layer renamed, recoloured, or locked leaves it alone: the same
    /// entities are shown, in the same places, with the same properties.
    /// Derived state that depends on the entities but not on what their
    /// layers look like can tell by this that it is still good.
    pub fn shown_revision(&self) -> Revision {
        self.shown_revision
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

    /// The linetype table. It always holds `Continuous`.
    pub fn linetypes(&self) -> &LinetypeTable {
        &self.linetypes
    }

    /// The drawing's settings.
    pub fn settings(&self) -> DrawingSettings {
        self.settings
    }

    /// The layer new objects are put on. It always exists and is on.
    pub fn current_layer(&self) -> LayerId {
        self.settings.current_layer
    }

    /// The colour new objects are given.
    pub fn current_color(&self) -> Color {
        self.settings.current_color
    }

    /// The linetype new objects are given.
    pub fn current_linetype(&self) -> Linetype {
        self.settings.current_linetype
    }

    /// The lineweight new objects are given.
    pub fn current_lineweight(&self) -> Lineweight {
        self.settings.current_lineweight
    }

    /// What every linetype pattern is multiplied by.
    pub fn ltscale(&self) -> f64 {
        self.settings.ltscale
    }

    /// The drawing's unit, or `None` if it is unitless.
    pub fn unit(&self) -> Option<LengthUnit> {
        self.settings.unit
    }

    /// The same drawing with its unit set, and still no history: for a
    /// drawing being put together from a file.
    #[must_use]
    pub fn with_unit(mut self, unit: Option<LengthUnit>) -> Self {
        self.settings.unit = unit;
        self
    }

    /// The layer an entity is on.
    pub fn layer_of(&self, id: EntityId) -> Option<&Layer> {
        self.layers.get(self.properties(id)?.layer)
    }

    /// Whether the entity is shown: it exists and its layer is on. What is
    /// not shown is not there for the pointer either — not picked, not
    /// selected, not snapped to.
    pub fn is_shown(&self, id: EntityId) -> bool {
        self.layer_of(id).is_some_and(|layer| layer.on)
    }

    /// Whether the entity is protected from change: its layer is locked.
    pub fn is_locked(&self, id: EntityId) -> bool {
        self.layer_of(id).is_some_and(|layer| layer.locked)
    }

    /// Every entity that is shown, in dense storage order.
    pub fn shown(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities
            .records()
            .filter(|(_, record)| {
                self.layers
                    .get(record.properties.layer)
                    .is_some_and(|layer| layer.on)
            })
            .map(|(id, record)| (id, &*record.entity))
    }

    /// [`query`](Self::query), keeping only entities that are shown. This is
    /// the query for anything the pointer does.
    pub fn query_shown(&self, bounds: &Aabb) -> Vec<EntityId> {
        let mut ids = self.index.query(bounds);
        ids.retain(|id| self.is_shown(*id));
        ids
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
        let mut shown_changed = false;
        for change in transaction.changes() {
            shown_changed |= match change {
                Change::Entity(_) => true,
                Change::Layer(TableChange::Replace { before, after, .. }) => before.on != after.on,
                // A layer that is made or removed has nothing on it.
                Change::Layer(_) | Change::Linetype(_) | Change::Settings(_) => false,
            };
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
                Change::Linetype(change) => {
                    self.linetypes.apply(change);
                    report.tables_changed = true;
                }
                Change::Settings(change) => self.settings = change.after,
            }
        }
        self.revision = Revision::next();
        if shown_changed {
            self.shown_revision = Revision::next();
        }
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
    /// The entity gets the properties new objects are given: the current
    /// layer, colour, linetype, and lineweight. An object made *from*
    /// another — a copy, an offset, the second piece of a break — takes
    /// that object's properties with [`insert_like`](Self::insert_like)
    /// instead.
    pub fn insert(&mut self, entity: impl Into<Entity>) -> EntityId {
        let settings = self.settings_now();
        let properties = Properties {
            layer: settings.current_layer,
            color: settings.current_color,
            linetype: settings.current_linetype,
            lineweight: settings.current_lineweight,
        };
        self.insert_record(EntityRecord::new(entity, properties))
    }

    /// Adds a new entity with the given properties.
    pub fn insert_with(
        &mut self,
        entity: impl Into<Entity>,
        properties: Properties,
    ) -> Result<EntityId, TransactionError> {
        self.known_layer(properties.layer)?;
        self.known_linetype(properties.linetype)?;
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
        self.known_linetype(properties.linetype)?;
        let before = self.existing(id)?;
        let after = before.with_properties(properties);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Adds a layer and returns its ID. Its name must be allowed and not
    /// taken, and its linetype must exist. Like an entity's, the ID is
    /// consumed even if the transaction is abandoned.
    pub fn add_layer(&mut self, layer: Layer) -> Result<LayerId, TransactionError> {
        self.free_name(&layer.name, None)?;
        self.known_linetype(layer.linetype)?;
        let id = self.drawing.layers.allocate();
        self.changes
            .push(Change::Layer(TableChange::Insert { id, row: layer }));
        Ok(id)
    }

    /// Replaces an existing layer's row — its name or any of its settings —
    /// keeping its ID, and so every entity on it.
    ///
    /// Layer `0` keeps its name, the current layer stays on, and the
    /// layer's linetype must exist.
    pub fn replace_layer(&mut self, id: LayerId, layer: Layer) -> Result<(), TransactionError> {
        let before = self.existing_layer(id)?;
        if layer.name != before.name {
            if id == LayerId::FIRST {
                return Err(TransactionError::PermanentLayer);
            }
            self.free_name(&layer.name, Some(id))?;
        }
        self.known_linetype(layer.linetype)?;
        if !layer.on && self.settings_now().current_layer == id {
            return Err(TransactionError::CurrentLayer(id));
        }
        self.changes.push(Change::Layer(TableChange::Replace {
            id,
            before,
            after: layer,
        }));
        Ok(())
    }

    /// Removes a layer that has no entities on it and is not the current
    /// one. Layer `0` stays.
    pub fn remove_layer(&mut self, id: LayerId) -> Result<(), TransactionError> {
        if id == LayerId::FIRST {
            return Err(TransactionError::PermanentLayer);
        }
        let row = self.existing_layer(id)?;
        if self.settings_now().current_layer == id {
            return Err(TransactionError::CurrentLayer(id));
        }
        // In use by an entity of the drawing, or by one this transaction
        // puts on it. (An entity this transaction takes *off* it still
        // counts: remove the layer in a transaction of its own.)
        let on_layer = |record: &EntityRecord| record.properties.layer == id;
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => on_layer(record),
            Change::Entity(EntityChange::Replace { after, .. }) => on_layer(after),
            Change::Entity(EntityChange::Remove { .. })
            | Change::Layer(_)
            | Change::Linetype(_)
            | Change::Settings(_) => false,
        });
        if pending || self.drawing.records().any(|(_, record)| on_layer(record)) {
            return Err(TransactionError::LayerInUse(id));
        }
        self.changes
            .push(Change::Layer(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Makes `id` the layer new objects are put on. It must exist and be on.
    pub fn set_current_layer(&mut self, id: LayerId) -> Result<(), TransactionError> {
        let layer = self
            .layer_now(id)
            .ok_or(TransactionError::UnknownLayer(id))?;
        if !layer.on {
            return Err(TransactionError::LayerIsOff(id));
        }
        self.set_settings(DrawingSettings {
            current_layer: id,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Sets the colour new objects are given. `ByLayer`, the default, means
    /// each takes its layer's.
    pub fn set_current_color(&mut self, color: Color) {
        self.set_settings(DrawingSettings {
            current_color: color,
            ..self.settings_now()
        });
    }

    /// Sets the lineweight new objects are given. `ByLayer`, the default,
    /// means each takes its layer's.
    pub fn set_current_lineweight(&mut self, lineweight: Lineweight) {
        self.set_settings(DrawingSettings {
            current_lineweight: lineweight,
            ..self.settings_now()
        });
    }

    /// Loads a linetype and returns its ID. It must be valid and its name
    /// not taken. Like a layer's, the ID is consumed even if the
    /// transaction is abandoned.
    pub fn add_linetype(&mut self, linetype: LinetypeDef) -> Result<LinetypeId, TransactionError> {
        linetype.check().map_err(TransactionError::BadLinetype)?;
        let added = self.changes.iter().filter_map(|change| match change {
            Change::Linetype(TableChange::Insert { row, .. }) => Some(row),
            _ => None,
        });
        let taken = self
            .drawing
            .linetypes
            .iter()
            .filter(|(id, _)| self.linetype_now(*id).is_some())
            .map(|(id, row)| (Some(id), row))
            .chain(added.map(|row| (None, row)))
            .find(|(_, row)| same_name(&row.name, &linetype.name));
        if let Some((id, _)) = taken {
            // A row added earlier in this transaction has no ID to report
            // that the drawing knows; its name will do.
            return Err(TransactionError::DuplicateLinetypeName(
                id.unwrap_or(LinetypeId::FIRST),
            ));
        }
        let id = self.drawing.linetypes.allocate();
        self.changes
            .push(Change::Linetype(TableChange::Insert { id, row: linetype }));
        Ok(id)
    }

    /// Removes a linetype that nothing refers to: no entity, no layer, and
    /// not the setting for new objects. `Continuous` stays.
    pub fn remove_linetype(&mut self, id: LinetypeId) -> Result<(), TransactionError> {
        if id == LinetypeId::FIRST {
            return Err(TransactionError::PermanentLinetype);
        }
        if self.linetype_change(id).is_some() {
            return Err(TransactionError::LinetypeAlreadyChanged(id));
        }
        let row = self
            .drawing
            .linetypes
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLinetype(id))?;
        let named = Linetype::Named(id);
        // In use in the drawing, or by what this transaction adds or
        // changes. (What this transaction takes *off* it still counts:
        // remove the linetype in a transaction of its own.)
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => {
                record.properties.linetype == named
            }
            Change::Entity(EntityChange::Replace { after, .. }) => {
                after.properties.linetype == named
            }
            Change::Layer(TableChange::Insert { row, .. }) => row.linetype == named,
            Change::Layer(TableChange::Replace { after, .. }) => after.linetype == named,
            _ => false,
        });
        let in_use = pending
            || self.settings_now().current_linetype == named
            || self
                .drawing
                .records()
                .any(|(_, record)| record.properties.linetype == named)
            || self
                .drawing
                .layers
                .iter()
                .any(|(_, layer)| layer.linetype == named);
        if in_use {
            return Err(TransactionError::LinetypeInUse(id));
        }
        self.changes
            .push(Change::Linetype(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Sets the linetype new objects are given. `ByLayer`, the default,
    /// means each takes its layer's. A named linetype must exist.
    pub fn set_current_linetype(&mut self, linetype: Linetype) -> Result<(), TransactionError> {
        self.known_linetype(linetype)?;
        self.set_settings(DrawingSettings {
            current_linetype: linetype,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Sets what every linetype pattern is multiplied by. It must be a
    /// positive number.
    pub fn set_ltscale(&mut self, ltscale: f64) -> Result<(), TransactionError> {
        if !(ltscale.is_finite() && ltscale > 0.0) {
            return Err(TransactionError::BadLtscale);
        }
        self.set_settings(DrawingSettings {
            ltscale,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Says what the drawing's numbers are numbers of; `None` makes it
    /// unitless. No geometry changes: the unit is what the numbers mean.
    pub fn set_unit(&mut self, unit: Option<LengthUnit>) {
        self.set_settings(DrawingSettings {
            unit,
            ..self.settings_now()
        });
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
            Change::Layer(_) | Change::Linetype(_) | Change::Settings(_) => false,
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

    /// The layer `id` as it will be once this transaction commits.
    fn layer_now(&self, id: LayerId) -> Option<&Layer> {
        match self.layer_change(id) {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.layers.get(id),
        }
    }

    /// How this transaction has changed the linetype `id` so far, if it has.
    fn linetype_change(&self, id: LinetypeId) -> Option<&TableChange<LinetypeDef>> {
        self.changes.iter().find_map(|change| match change {
            Change::Linetype(change) if change.id() == id => Some(change),
            _ => None,
        })
    }

    /// The linetype `id` as it will be once this transaction commits.
    fn linetype_now(&self, id: LinetypeId) -> Option<&LinetypeDef> {
        match self.linetype_change(id) {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.linetypes.get(id),
        }
    }

    /// Whether `linetype` may be given to an entity, a layer, or new
    /// objects: it is an indirection, or a row that exists once this
    /// transaction commits.
    fn known_linetype(&self, linetype: Linetype) -> Result<(), TransactionError> {
        match linetype {
            Linetype::Named(id) if self.linetype_now(id).is_none() => {
                Err(TransactionError::UnknownLinetype(id))
            }
            _ => Ok(()),
        }
    }

    /// The settings as they will be once this transaction commits.
    fn settings_now(&self) -> DrawingSettings {
        self.changes
            .iter()
            .rev()
            .find_map(|change| match change {
                Change::Settings(change) => Some(change.after),
                _ => None,
            })
            .unwrap_or(self.drawing.settings)
    }

    /// Records the settings as `after`. A transaction holds at most one
    /// settings change: a second one moves the first one's end.
    fn set_settings(&mut self, after: DrawingSettings) {
        let pending = self.changes.iter_mut().find_map(|change| match change {
            Change::Settings(change) => Some(change),
            _ => None,
        });
        match pending {
            Some(change) => change.after = after,
            None => self.changes.push(Change::Settings(SettingsChange {
                before: self.drawing.settings,
                after,
            })),
        }
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
        match self.layer_now(id) {
            Some(_) => Ok(()),
            None => Err(TransactionError::UnknownLayer(id)),
        }
    }

    /// Whether `name` can be given to a layer — to `renamed`, or to a new
    /// one: it is allowed, and no other layer has it once this transaction
    /// commits.
    fn free_name(&self, name: &str, renamed: Option<LayerId>) -> Result<(), TransactionError> {
        check_name(name).map_err(TransactionError::BadLayerName)?;
        let added = self.changes.iter().filter_map(|change| match change {
            Change::Layer(TableChange::Insert { id, .. }) => Some(*id),
            _ => None,
        });
        let taken = self
            .drawing
            .layers
            .iter()
            .map(|(id, _)| id)
            .chain(added)
            .filter(|id| Some(*id) != renamed)
            .find(|id| {
                self.layer_now(*id)
                    .is_some_and(|layer| same_name(&layer.name, name))
            });
        match taken {
            Some(id) => Err(TransactionError::DuplicateLayerName(id)),
            None => Ok(()),
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
        let id = tx.add_layer(Layer::named(name)).unwrap();
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
            [],
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
        let layer = tx.add_layer(Layer::named("walls")).unwrap();
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

    // ── layer settings, names, and the current layer (#132) ──

    fn change_layer(
        drawing: &mut Drawing,
        id: LayerId,
        edit: impl FnOnce(&mut Layer),
    ) -> Result<(), TransactionError> {
        let mut layer = drawing.layers().get(id).cloned().expect("the layer exists");
        edit(&mut layer);
        let mut tx = drawing.transaction();
        let result = tx.replace_layer(id, layer);
        tx.commit();
        result
    }

    #[test]
    fn each_setting_of_a_layer_is_one_undoable_change() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let fresh = Layer::named("walls");

        type Edit = fn(&mut Layer);
        let edits: [(&str, Edit); 6] = [
            ("rename", |layer| layer.name = "partitions".to_owned()),
            ("colour", |layer| layer.color = crate::Color::Index(3)),
            ("linetype", |layer| {
                layer.linetype = crate::Linetype::ByBlock
            }),
            ("lineweight", |layer| {
                layer.lineweight = crate::Lineweight::ByBlock
            }),
            ("off", |layer| layer.on = false),
            ("lock", |layer| layer.locked = true),
        ];
        for (what, edit) in edits {
            change_layer(&mut drawing, walls, edit).unwrap();
            let mut expected = fresh.clone();
            edit(&mut expected);
            assert_eq!(drawing.layers().get(walls), Some(&expected), "{what}");
            drawing.undo().unwrap();
            assert_eq!(drawing.layers().get(walls), Some(&fresh), "{what} undone");
            drawing.redo().unwrap();
            assert_eq!(
                drawing.layers().get(walls),
                Some(&expected),
                "{what} redone"
            );
            drawing.undo().unwrap();
        }
    }

    #[test]
    fn layer_zero_keeps_its_name_but_takes_every_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(
            change_layer(&mut drawing, LayerId::FIRST, |layer| layer.name =
                "base".to_owned()),
            Err(TransactionError::PermanentLayer)
        );
        change_layer(&mut drawing, LayerId::FIRST, |layer| {
            layer.color = crate::Color::Index(2);
            layer.locked = true;
        })
        .unwrap();
        let zero = drawing.layers().get(LayerId::FIRST).unwrap();
        assert_eq!(
            (zero.name.as_str(), zero.color, zero.locked),
            ("0", crate::Color::Index(2), true)
        );
    }

    #[test]
    fn layer_names_must_be_allowed_and_unique_whatever_their_case() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "Walls");
        let doors = add_layer(&mut drawing, "doors");

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.add_layer(Layer::named("WALLS")),
            Err(TransactionError::DuplicateLayerName(walls))
        );
        assert_eq!(
            tx.add_layer(Layer::named("0")),
            Err(TransactionError::DuplicateLayerName(LayerId::FIRST))
        );
        assert_eq!(
            tx.add_layer(Layer::named("a*b")),
            Err(TransactionError::BadLayerName(NameError::Forbidden('*')))
        );
        assert_eq!(
            tx.add_layer(Layer::named("")),
            Err(TransactionError::BadLayerName(NameError::Empty))
        );
        // Two new layers in one transaction cannot share a name either.
        let windows = tx.add_layer(Layer::named("windows")).unwrap();
        assert_eq!(
            tx.add_layer(Layer::named("Windows")),
            Err(TransactionError::DuplicateLayerName(windows))
        );
        tx.commit();

        assert_eq!(
            change_layer(&mut drawing, doors, |layer| layer.name = "walls".to_owned()),
            Err(TransactionError::DuplicateLayerName(walls))
        );
        assert_eq!(
            change_layer(&mut drawing, doors, |layer| layer.name =
                " doors".to_owned()),
            Err(TransactionError::BadLayerName(NameError::Padded))
        );
        // Changing only the case of a layer's own name is a rename like any other.
        change_layer(&mut drawing, doors, |layer| layer.name = "Doors".to_owned()).unwrap();
        assert_eq!(drawing.layers().find("DOORS"), Some(doors));

        // A name freed in a transaction can be taken in the same one.
        let mut tx = drawing.transaction();
        tx.replace_layer(walls, Layer::named("partitions")).unwrap();
        let new_walls = tx.add_layer(Layer::named("walls")).unwrap();
        tx.commit();
        assert_eq!(drawing.layers().find("walls"), Some(new_walls));
        assert_eq!(drawing.layers().find("partitions"), Some(walls));
    }

    #[test]
    fn new_entities_go_on_the_current_layer_and_changing_it_is_undoable() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        assert_eq!(drawing.current_layer(), LayerId::FIRST);

        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.current_layer(), walls);
        assert_eq!(
            drawing.settings(),
            DrawingSettings {
                current_layer: walls,
                ..DrawingSettings::default()
            }
        );
        let on_walls = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));
        assert_eq!(drawing.properties(on_walls), Some(Properties::on(walls)));

        drawing.undo().unwrap(); // the line
        drawing.undo().unwrap(); // the setting
        assert_eq!(drawing.current_layer(), LayerId::FIRST);
        let on_zero = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(drawing.properties(on_zero), Some(Properties::default()));

        // Set and draw in one transaction: the entity is on the new layer,
        // and setting twice records one change from the first value to the last.
        let doors = add_layer(&mut drawing, "doors");
        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.set_current_layer(doors).unwrap();
        let id = tx.insert(line(0.0, 9.0, 10.0, 9.0));
        tx.commit();
        assert_eq!(drawing.properties(id), Some(Properties::on(doors)));
        drawing.undo().unwrap();
        assert_eq!(
            drawing.current_layer(),
            LayerId::FIRST,
            "one step back to where it was"
        );
    }

    /// #135: new objects take the current colour; it is a setting, undone
    /// like any other, and an object keeps what it was given.
    #[test]
    fn new_entities_take_the_current_colour_which_is_an_undoable_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.current_color(), Color::ByLayer);
        let walls = add_layer(&mut drawing, "walls");
        let by_layer = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let mut tx = drawing.transaction();
        tx.set_current_color(Color::Index(1));
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.current_color(), Color::Index(1));
        let red = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(
            drawing.properties(red),
            Some(Properties {
                color: Color::Index(1),
                ..Properties::on(walls)
            })
        );
        assert_eq!(
            drawing.properties(by_layer),
            Some(Properties::default()),
            "untouched"
        );

        // An object made from another takes that one's colour, not the current one.
        let mut tx = drawing.transaction();
        let copy = tx.insert_like(by_layer, line(0.0, 9.0, 10.0, 9.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.properties(copy).unwrap().color, Color::ByLayer);

        drawing.undo().unwrap(); // the copy
        drawing.undo().unwrap(); // the red line
        drawing.undo().unwrap(); // the settings: colour and layer together
        assert_eq!(
            (drawing.current_color(), drawing.current_layer()),
            (Color::ByLayer, LayerId::FIRST)
        );
        drawing.redo().unwrap();
        assert_eq!(drawing.current_color(), Color::Index(1));

        // Set and draw in one transaction.
        let mut tx = drawing.transaction();
        tx.set_current_color(Color::ByBlock);
        let id = tx.insert(line(0.0, 20.0, 10.0, 20.0));
        tx.commit();
        assert_eq!(drawing.properties(id).unwrap().color, Color::ByBlock);
    }

    /// #136: the same for the lineweight new objects are given.
    #[test]
    fn new_entities_take_the_current_lineweight_which_is_an_undoable_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.current_lineweight(), Lineweight::ByLayer);
        let thin = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let heavy = Lineweight::Hundredths(100);
        let mut tx = drawing.transaction();
        tx.set_current_lineweight(heavy);
        tx.commit();
        let wide = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(drawing.properties(wide).unwrap().lineweight, heavy);
        assert_eq!(
            drawing.properties(thin).unwrap().lineweight,
            Lineweight::ByLayer
        );
        assert_eq!(
            drawing.current_color(),
            Color::ByLayer,
            "the other settings are kept"
        );

        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(drawing.current_lineweight(), Lineweight::ByLayer);
        drawing.redo().unwrap();
        assert_eq!(drawing.current_lineweight(), heavy);
    }

    /// #113: changing the drawing unit is undoable, and changes no geometry.
    #[test]
    fn the_drawing_unit_is_a_setting_changed_and_undone_like_any_other() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.unit(), None, "a new drawing is unitless");
        let id = create(&mut drawing, line(0.0, 0.0, 25.0, 0.0));
        let walls = add_layer(&mut drawing, "walls");

        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Millimetres));
        tx.commit();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 25.0, 0.0))),
            "the unit says what the numbers mean; it does not change them"
        );

        // The unit and the current layer are one settings record: each
        // change keeps the other.
        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Metres));
        tx.commit();
        assert_eq!(drawing.current_layer(), walls);

        drawing.undo().unwrap();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(drawing.unit(), None);
        drawing.redo().unwrap();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));

        // Both in one transaction: one change, one step.
        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Feet));
        tx.set_current_layer(walls).unwrap();
        tx.set_unit(None);
        tx.commit();
        assert_eq!((drawing.unit(), drawing.current_layer()), (None, walls));
        drawing.undo().unwrap();
        assert_eq!(
            (drawing.unit(), drawing.current_layer()),
            (Some(LengthUnit::Millimetres), LayerId::FIRST)
        );

        let imported = Drawing::new().with_unit(Some(LengthUnit::Inches));
        assert_eq!(imported.unit(), Some(LengthUnit::Inches));
        assert!(!imported.can_undo());
    }

    #[test]
    fn the_current_layer_cannot_be_removed_switched_off_or_be_an_off_layer() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let hidden = add_layer(&mut drawing, "hidden");
        change_layer(&mut drawing, hidden, |layer| layer.on = false).unwrap();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.set_current_layer(hidden),
            Err(TransactionError::LayerIsOff(hidden))
        );
        assert_eq!(
            tx.set_current_layer(LayerId::from_raw(50)),
            Err(TransactionError::UnknownLayer(LayerId::from_raw(50)))
        );
        tx.set_current_layer(walls).unwrap();
        // Already current within this transaction.
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::CurrentLayer(walls))
        );
        tx.commit();

        assert_eq!(
            change_layer(&mut drawing, walls, |layer| layer.on = false),
            Err(TransactionError::CurrentLayer(walls))
        );
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::CurrentLayer(walls))
        );
        drop(tx);
        // Locking the current layer is allowed: new objects can still be drawn on it.
        change_layer(&mut drawing, walls, |layer| layer.locked = true).unwrap();
        assert_eq!(drawing.current_layer(), walls);
    }

    /// #134: what is shown has its own revision, which a change to how a
    /// layer looks does not touch.
    #[test]
    fn the_shown_revision_changes_with_entities_and_layer_visibility_only() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let expect = |drawing: &mut Drawing,
                      changes: bool,
                      what: &str,
                      edit: &mut dyn FnMut(&mut Drawing)| {
            let (shown, all) = (drawing.shown_revision(), drawing.revision());
            edit(drawing);
            assert_ne!(drawing.revision(), all, "{what}: the drawing changed");
            assert_eq!(drawing.shown_revision() != shown, changes, "{what}");
        };

        expect(&mut drawing, false, "a new layer", &mut |d| {
            add_layer(d, "doors");
        });
        expect(&mut drawing, false, "a layer's colour", &mut |d| {
            change_layer(d, walls, |layer| layer.color = crate::Color::Index(1)).unwrap();
        });
        expect(&mut drawing, false, "a layer's name", &mut |d| {
            change_layer(d, walls, |layer| layer.name = "partitions".to_owned()).unwrap();
        });
        expect(&mut drawing, false, "a layer locked", &mut |d| {
            change_layer(d, walls, |layer| layer.locked = true).unwrap();
        });
        expect(&mut drawing, false, "the current layer", &mut |d| {
            let mut tx = d.transaction();
            tx.set_unit(Some(LengthUnit::Metres));
            tx.commit();
        });
        expect(&mut drawing, false, "undoing a setting", &mut |d| {
            d.undo().unwrap();
        });

        expect(&mut drawing, true, "a layer switched off", &mut |d| {
            change_layer(d, walls, |layer| layer.on = false).unwrap();
        });
        expect(&mut drawing, true, "undoing that", &mut |d| {
            d.undo().unwrap();
        });
        expect(&mut drawing, true, "an entity drawn", &mut |d| {
            create(d, line(0.0, 5.0, 10.0, 5.0));
        });
        expect(&mut drawing, true, "an entity moved", &mut |d| {
            let mut tx = d.transaction();
            tx.replace(id, line(1.0, 1.0, 9.0, 9.0)).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "an entity's properties", &mut |d| {
            let mut tx = d.transaction();
            tx.set_properties(id, Properties::on(walls)).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "an entity erased", &mut |d| {
            let mut tx = d.transaction();
            tx.remove(id).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "redo after undo", &mut |d| {
            d.undo().unwrap();
        });
    }

    // ── linetypes (#138) ──

    fn load(drawing: &mut Drawing, name: &str) -> LinetypeId {
        let mut tx = drawing.transaction();
        let id = tx
            .add_linetype(LinetypeDef::built_in_named(name).expect("a built-in linetype"))
            .unwrap();
        tx.commit();
        id
    }

    #[test]
    fn linetypes_are_loaded_once_each_and_continuous_stays() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.linetypes().len(), 1);
        let dashed = load(&mut drawing, "Dashed");
        assert_eq!(dashed, LinetypeId::from_raw(1));
        assert_eq!(drawing.linetypes().find("dashed"), Some(dashed));

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.add_linetype(LinetypeDef::built_in_named("DASHED").unwrap()),
            Err(TransactionError::DuplicateLinetypeName(dashed))
        );
        assert_eq!(
            tx.add_linetype(LinetypeDef {
                name: "continuous".to_owned(),
                ..LinetypeDef::built_in_named("Hidden").unwrap()
            }),
            Err(TransactionError::DuplicateLinetypeName(LinetypeId::FIRST))
        );
        assert_eq!(
            tx.add_linetype(LinetypeDef {
                pattern: vec![0.5],
                ..LinetypeDef::built_in_named("Hidden").unwrap()
            }),
            Err(TransactionError::BadLinetype(LinetypeError::Odd))
        );
        // Two of the same name in one transaction: the second is refused.
        tx.add_linetype(LinetypeDef::built_in_named("Hidden").unwrap())
            .unwrap();
        assert!(matches!(
            tx.add_linetype(LinetypeDef::built_in_named("hidden").unwrap()),
            Err(TransactionError::DuplicateLinetypeName(_))
        ));
        assert_eq!(
            tx.remove_linetype(LinetypeId::FIRST),
            Err(TransactionError::PermanentLinetype)
        );
        tx.commit();
        assert_eq!(drawing.linetypes().len(), 3);

        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 2, "one transaction, one step");
        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 1);
        drawing.redo();
        drawing.redo();
        assert_eq!(
            drawing.linetypes().find("hidden"),
            Some(LinetypeId::from_raw(2))
        );
        // IDs are never reused, even after an undo.
        drawing.undo();
        drawing.undo();
        assert_eq!(load(&mut drawing, "Center"), LinetypeId::from_raw(3));
    }

    #[test]
    fn a_linetype_in_use_cannot_be_removed() {
        let mut drawing = Drawing::new();
        let dashed = load(&mut drawing, "Dashed");
        let hidden = load(&mut drawing, "Hidden");
        let center = load(&mut drawing, "Center");
        let phantom = load(&mut drawing, "Phantom");

        // By an entity.
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(
                line(0.0, 0.0, 1.0, 0.0),
                Properties {
                    linetype: Linetype::Named(dashed),
                    ..Properties::default()
                },
            )
            .unwrap();
        tx.commit();
        // By a layer.
        let mut tx = drawing.transaction();
        let walls = tx
            .add_layer(Layer {
                linetype: Linetype::Named(hidden),
                ..Layer::named("walls")
            })
            .unwrap();
        tx.commit();
        // As the setting for new objects.
        let mut tx = drawing.transaction();
        tx.set_current_linetype(Linetype::Named(center)).unwrap();
        tx.commit();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_linetype(dashed),
            Err(TransactionError::LinetypeInUse(dashed))
        );
        assert_eq!(
            tx.remove_linetype(hidden),
            Err(TransactionError::LinetypeInUse(hidden))
        );
        assert_eq!(
            tx.remove_linetype(center),
            Err(TransactionError::LinetypeInUse(center))
        );
        assert_eq!(
            tx.remove_linetype(LinetypeId::from_raw(40)),
            Err(TransactionError::UnknownLinetype(LinetypeId::from_raw(40)))
        );
        // Given to something in this very transaction counts too.
        tx.set_properties(
            id,
            Properties {
                linetype: Linetype::Named(phantom),
                ..Properties::default()
            },
        )
        .unwrap();
        assert_eq!(
            tx.remove_linetype(phantom),
            Err(TransactionError::LinetypeInUse(phantom))
        );
        drop(tx);

        // Taken off everything, it goes — in a transaction of its own.
        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::default()).unwrap();
        tx.replace_layer(walls, Layer::named("walls")).unwrap();
        tx.set_current_linetype(Linetype::ByLayer).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove_linetype(dashed).unwrap();
        tx.remove_linetype(hidden).unwrap();
        tx.remove_linetype(center).unwrap();
        assert_eq!(
            tx.remove_linetype(dashed),
            Err(TransactionError::LinetypeAlreadyChanged(dashed))
        );
        tx.commit();
        assert_eq!(drawing.linetypes().len(), 2);
        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 5);
    }

    #[test]
    fn nothing_can_refer_to_a_linetype_that_does_not_exist() {
        let mut drawing = Drawing::new();
        let gone = LinetypeId::from_raw(9);
        let unknown = TransactionError::UnknownLinetype(gone);
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_current_linetype(Linetype::Named(gone)), Err(unknown));
        assert_eq!(
            tx.add_layer(Layer {
                linetype: Linetype::Named(gone),
                ..Layer::named("walls")
            }),
            Err(unknown)
        );
        assert_eq!(
            tx.replace_layer(
                LayerId::FIRST,
                Layer {
                    linetype: Linetype::Named(gone),
                    ..Layer::named("0")
                }
            ),
            Err(unknown)
        );
        let dotted = Properties {
            linetype: Linetype::Named(gone),
            ..Properties::default()
        };
        assert_eq!(
            tx.insert_with(line(0.0, 0.0, 1.0, 0.0), dotted),
            Err(unknown)
        );
        let id = tx.insert(line(0.0, 0.0, 1.0, 0.0));
        tx.commit();
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_properties(id, dotted), Err(unknown));
        // A linetype loaded in this transaction can be used in it.
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("Dashed").unwrap())
            .unwrap();
        tx.set_current_linetype(Linetype::Named(dashed)).unwrap();
        let second = tx.insert(line(0.0, 1.0, 1.0, 1.0));
        tx.commit();
        assert_eq!(
            drawing.properties(second).unwrap().linetype,
            Linetype::Named(dashed),
            "new objects are given the current linetype"
        );
        assert_eq!(drawing.properties(id).unwrap().linetype, Linetype::ByLayer);

        assert_eq!(
            Drawing::from_parts(
                [Layer {
                    linetype: Linetype::Named(gone),
                    ..Layer::named("walls")
                }],
                [],
                []
            )
            .err(),
            Some(TransactionError::UnknownLinetype(gone))
        );
        assert_eq!(
            Drawing::from_parts([], [], [(Entity::Line(line(0.0, 0.0, 1.0, 0.0)), dotted)]).err(),
            Some(TransactionError::UnknownLinetype(gone))
        );
        let loaded = Drawing::from_parts(
            [],
            [LinetypeDef::built_in_named("Hidden").unwrap()],
            [(
                Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
                Properties {
                    linetype: Linetype::Named(LinetypeId::from_raw(1)),
                    ..Properties::default()
                },
            )],
        )
        .unwrap();
        assert_eq!(
            loaded.linetypes().find("hidden"),
            Some(LinetypeId::from_raw(1))
        );
        assert!(!loaded.can_undo());
    }

    #[test]
    fn ltscale_is_a_positive_setting_with_its_own_undo() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.ltscale(), 1.0);
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_ltscale(0.0), Err(TransactionError::BadLtscale));
        assert_eq!(tx.set_ltscale(-2.0), Err(TransactionError::BadLtscale));
        assert_eq!(tx.set_ltscale(f64::NAN), Err(TransactionError::BadLtscale));
        assert_eq!(
            tx.set_ltscale(f64::INFINITY),
            Err(TransactionError::BadLtscale)
        );
        tx.set_ltscale(2.5).unwrap();
        tx.commit();
        assert_eq!(drawing.ltscale(), 2.5);
        let shown = drawing.shown_revision();
        drawing.undo();
        assert_eq!(drawing.ltscale(), 1.0);
        assert_eq!(
            drawing.shown_revision(),
            shown,
            "a setting changes no geometry"
        );
        drawing.redo();
        assert_eq!(drawing.ltscale(), 2.5);
    }

    #[test]
    fn what_is_on_an_off_layer_is_not_shown_and_what_is_on_a_locked_one_is_locked() {
        let (off, locked) = (LayerId::from_raw(1), LayerId::from_raw(2));
        let drawing = Drawing::from_parts(
            [
                Layer {
                    on: false,
                    ..Layer::named("off")
                },
                Layer {
                    locked: true,
                    ..Layer::named("locked")
                },
            ],
            [],
            [
                (
                    Entity::Line(line(0.0, 0.0, 10.0, 0.0)),
                    Properties::default(),
                ),
                (Entity::Line(line(0.0, 1.0, 10.0, 1.0)), Properties::on(off)),
                (
                    Entity::Line(line(0.0, 2.0, 10.0, 2.0)),
                    Properties::on(locked),
                ),
            ],
        )
        .unwrap();
        let ids = live_ids(&drawing);
        assert_eq!(
            ids.iter()
                .map(|id| drawing.is_shown(*id))
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(
            ids.iter()
                .map(|id| drawing.is_locked(*id))
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(
            drawing.layer_of(ids[1]).map(|layer| layer.name.as_str()),
            Some("off")
        );
        assert_eq!(
            sorted(drawing.query_shown(&everything())),
            vec![ids[0], ids[2]]
        );
        assert_eq!(
            sorted(drawing.query(&everything())),
            ids,
            "the index itself holds everything"
        );
        assert_eq!(
            sorted(drawing.shown().map(|(id, _)| id).collect()),
            vec![ids[0], ids[2]]
        );
        let ghost = EntityId::from_raw(999);
        assert!(!drawing.is_shown(ghost) && !drawing.is_locked(ghost));

        let duplicate = Drawing::from_parts([Layer::named("a"), Layer::named("A")], [], []);
        assert_eq!(
            duplicate.err(),
            Some(TransactionError::DuplicateLayerName(LayerId::from_raw(1)))
        );
        let bad = Drawing::from_parts([Layer::named("a,b")], [], []);
        assert_eq!(
            bad.err(),
            Some(TransactionError::BadLayerName(NameError::Forbidden(',')))
        );
    }

    #[derive(Clone, Debug)]
    enum Op {
        Create(f64, f64, f64, f64),
        CreateOn(f64, usize),
        Erase(usize),
        Modify(usize, f64, f64),
        Restyle(usize, usize, u8),
        AddLayer(u8),
        RenameLayer(usize, u8),
        SwitchLayer(usize, bool, bool),
        RemoveLayer(usize),
        SetCurrent(usize),
        SetUnit(usize),
        /// A built-in linetype, by index.
        LoadLinetype(usize),
        RemoveLinetype(usize),
        /// An entity given a linetype; a layer given one; the setting.
        Dash(usize, usize),
        DashLayer(usize, usize),
        SetCurrentLinetype(usize),
        SetLtscale(f64),
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
            3 => (coordinate(), coordinate(), coordinate(), coordinate())
                .prop_map(|(a, b, c, d)| Op::Create(a, b, c, d)),
            2 => (coordinate(), any::<usize>()).prop_map(|(x, layer)| Op::CreateOn(x, layer)),
            1 => any::<usize>().prop_map(Op::Erase),
            2 => (any::<usize>(), coordinate(), coordinate()).prop_map(|(i, x, y)| Op::Modify(i, x, y)),
            2 => (any::<usize>(), any::<usize>(), 1..8u8).prop_map(|(i, layer, color)| Op::Restyle(i, layer, color)),
            2 => (0..12u8).prop_map(Op::AddLayer),
            1 => (any::<usize>(), 0..12u8).prop_map(|(i, name)| Op::RenameLayer(i, name)),
            1 => (any::<usize>(), any::<bool>(), any::<bool>()).prop_map(|(i, on, locked)| Op::SwitchLayer(i, on, locked)),
            1 => any::<usize>().prop_map(Op::RemoveLayer),
            1 => any::<usize>().prop_map(Op::SetCurrent),
            1 => any::<usize>().prop_map(Op::SetUnit),
            2 => any::<usize>().prop_map(Op::LoadLinetype),
            1 => any::<usize>().prop_map(Op::RemoveLinetype),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(i, lt)| Op::Dash(i, lt)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(i, lt)| Op::DashLayer(i, lt)),
            1 => any::<usize>().prop_map(Op::SetCurrentLinetype),
            1 => (-1.0..5.0_f64).prop_map(Op::SetLtscale),
            1 => (any::<usize>(), coordinate()).prop_map(|(i, x)| Op::Together(i, x)),
            3 => Just(Op::Undo),
            2 => Just(Op::Redo),
        ]
    }

    /// Everything that is saved: every entity with its geometry and
    /// properties, every layer, every linetype, and the settings.
    type Saved = (
        Vec<(EntityId, Entity, Properties)>,
        Vec<(LayerId, Layer)>,
        Vec<(LinetypeId, LinetypeDef)>,
        DrawingSettings,
    );

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
        let linetypes = drawing
            .linetypes()
            .iter()
            .map(|(id, linetype)| (id, linetype.clone()))
            .collect();
        (entities, layers, linetypes, drawing.settings())
    }

    proptest! {
        /// After any sequence of edits to entities, layers, and settings,
        /// undos, and redos — refused changes included:
        ///  - the spatial index equals one rebuilt from scratch (§19.5);
        ///  - no entity or layer ID is ever handed out twice (§10.2);
        ///  - the allocator never decreases;
        ///  - every entity is on a layer that exists; layer names are
        ///    unique; the current layer exists and is on;
        ///  - every linetype an entity, a layer, or the settings name
        ///    exists; linetype names are unique;
        ///  - undoing everything gives back the empty drawing, and redoing
        ///    everything gives back exactly the drawing there was (§19.2):
        ///    one history for entities, tables, and settings.
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
            let mut linetypes_made: Vec<LinetypeId> = Vec::new();
            let mut high_water = drawing.next_entity_id();
            let mut together = 0;

            for op in ops {
                let live = live_ids(&drawing);
                let layers: Vec<LayerId> = drawing.layers().iter().map(|(id, _)| id).collect();
                let layer = |pick: usize| layers[pick % layers.len()];
                let linetypes: Vec<LinetypeId> = drawing.linetypes().iter().map(|(id, _)| id).collect();
                // One past the table: a linetype that does not exist.
                let linetype = |pick: usize| {
                    let index = pick % (linetypes.len() + 1);
                    linetypes.get(index).copied().unwrap_or(LinetypeId::from_raw(u32::MAX))
                };
                let in_use = |drawing: &Drawing, id: LinetypeId| {
                    let named = Linetype::Named(id);
                    drawing.records().any(|(_, record)| record.properties.linetype == named)
                        || drawing.layers().iter().any(|(_, layer)| layer.linetype == named)
                        || drawing.current_linetype() == named
                };
                match op {
                    Op::Create(x0, y0, x1, y1) => {
                        let id = create(&mut drawing, line(x0, y0, x1, y1));
                        prop_assert!(!allocated.contains(&id), "ID {id} was reused");
                        prop_assert_eq!(drawing.properties(id).unwrap().layer, drawing.current_layer());
                        allocated.push(id);
                    }
                    Op::CreateOn(x, on) => {
                        let mut tx = drawing.transaction();
                        allocated.push(tx.insert_with(line(x, 0.0, x, 9.0), Properties::on(layer(on))).unwrap());
                        tx.commit();
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
                        // Few names: collisions are common and must be refused.
                        let name = format!("Layer {name}");
                        let taken = drawing.layers().find(&name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_layer(Layer::named(name.to_uppercase()));
                        tx.commit();
                        match (added, taken) {
                            (Ok(id), None) => {
                                prop_assert!(!layers_made.contains(&id), "layer ID {} was reused", id);
                                layers_made.push(id);
                            }
                            (Err(TransactionError::DuplicateLayerName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "adding a layer: {:?}", other),
                        }
                    }
                    Op::RenameLayer(pick, name) => {
                        let id = layer(pick);
                        let name = format!("layer {name}");
                        let taken = drawing.layers().find(&name).filter(|other| *other != id);
                        let renamed = change_layer(&mut drawing, id, |layer| layer.name = name);
                        prop_assert_eq!(renamed.is_ok(), id != LayerId::FIRST && taken.is_none(), "{:?}", renamed);
                    }
                    Op::SwitchLayer(pick, on, locked) => {
                        let id = layer(pick);
                        let switched = change_layer(&mut drawing, id, |layer| {
                            layer.on = on;
                            layer.locked = locked;
                        });
                        prop_assert_eq!(switched.is_ok(), on || id != drawing.current_layer(), "{:?}", switched);
                    }
                    Op::RemoveLayer(pick) => {
                        let id = layer(pick);
                        let in_use = drawing.records().any(|(_, record)| record.properties.layer == id);
                        let current = drawing.current_layer() == id;
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_layer(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != LayerId::FIRST && !in_use && !current);
                    }
                    Op::SetCurrent(pick) => {
                        let id = layer(pick);
                        let on = drawing.layers().get(id).unwrap().on;
                        let mut tx = drawing.transaction();
                        let set = tx.set_current_layer(id);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), on);
                    }
                    Op::SetUnit(pick) => {
                        let units = LengthUnit::ALL;
                        let unit = (pick % (units.len() + 1)).checked_sub(1).map(|index| units[index]);
                        let layer_before = drawing.current_layer();
                        let mut tx = drawing.transaction();
                        tx.set_unit(unit);
                        tx.commit();
                        prop_assert_eq!(drawing.unit(), unit);
                        prop_assert_eq!(drawing.current_layer(), layer_before);
                    }
                    Op::LoadLinetype(pick) => {
                        let built_in = LinetypeDef::built_in();
                        let def = built_in[pick % built_in.len()].clone();
                        let taken = drawing.linetypes().find(&def.name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_linetype(def);
                        tx.commit();
                        match (added, taken) {
                            (Ok(id), None) => {
                                prop_assert!(!linetypes_made.contains(&id), "linetype ID {} was reused", id);
                                linetypes_made.push(id);
                            }
                            (Err(TransactionError::DuplicateLinetypeName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "loading a linetype: {:?}", other),
                        }
                    }
                    Op::RemoveLinetype(pick) => {
                        let id = linetype(pick);
                        let exists = drawing.linetypes().contains(id);
                        let used = in_use(&drawing, id);
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_linetype(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), exists && id != LinetypeId::FIRST && !used, "{:?}", removed);
                    }
                    Op::Dash(pick, lt) if !live.is_empty() => {
                        let id = live[pick % live.len()];
                        let named = Linetype::Named(linetype(lt));
                        let before = drawing.properties(id).unwrap();
                        let mut tx = drawing.transaction();
                        let set = tx.set_properties(id, Properties { linetype: named, ..before });
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(lt)));
                    }
                    Op::DashLayer(pick, lt) => {
                        let named = Linetype::Named(linetype(lt));
                        let set = change_layer(&mut drawing, layer(pick), |layer| layer.linetype = named);
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(lt)), "{:?}", set);
                    }
                    Op::SetCurrentLinetype(pick) => {
                        let named = Linetype::Named(linetype(pick));
                        let mut tx = drawing.transaction();
                        let set = tx.set_current_linetype(named);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(pick)));
                    }
                    Op::SetLtscale(scale) => {
                        let mut tx = drawing.transaction();
                        let set = tx.set_ltscale(scale);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), scale > 0.0);
                        prop_assert!(drawing.ltscale() > 0.0);
                    }
                    Op::Together(pick, x) => {
                        together += 1;
                        let mut tx = drawing.transaction();
                        let new_layer = tx.add_layer(Layer::named(format!("together {together}"))).unwrap();
                        tx.set_current_layer(new_layer).unwrap();
                        allocated.push(tx.insert(line(x, 0.0, x, 5.0)));
                        if !live.is_empty() {
                            tx.set_properties(live[pick % live.len()], Properties::on(new_layer)).unwrap();
                        }
                        tx.commit();
                        layers_made.push(new_layer);
                    }
                    Op::Erase(_) | Op::Modify(..) | Op::Restyle(..) | Op::Dash(..) => {}
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
                let current = drawing.layers().get(drawing.current_layer());
                prop_assert!(current.is_some_and(|layer| layer.on), "the current layer is {:?}", current);
                let known = |linetype: Linetype| match linetype {
                    Linetype::Named(id) => drawing.linetypes().contains(id),
                    Linetype::ByLayer | Linetype::ByBlock => true,
                };
                for (id, record) in drawing.records() {
                    prop_assert!(known(record.properties.linetype), "entity {} has a linetype that does not exist", id);
                }
                for (id, layer) in drawing.layers().iter() {
                    prop_assert!(known(layer.linetype), "layer {} has a linetype that does not exist", id);
                }
                prop_assert!(known(drawing.current_linetype()));
                let mut linetype_names: Vec<String> = drawing.linetypes().iter().map(|(_, lt)| lt.name.to_lowercase()).collect();
                linetype_names.sort();
                let count = linetype_names.len();
                linetype_names.dedup();
                prop_assert_eq!(linetype_names.len(), count, "two linetypes share a name");
                let mut names: Vec<String> = drawing.layers().iter().map(|(_, layer)| layer.name.to_lowercase()).collect();
                names.sort();
                let count = names.len();
                names.dedup();
                prop_assert_eq!(names.len(), count, "two layers share a name");
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
