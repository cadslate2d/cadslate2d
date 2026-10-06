//! Reversible changes to the saved drawing (DESIGN §19).
//!
//! Every persisted mutation is a [`Change`] inside a [`Transaction`]. There is
//! one change model and one history for entities and, later, for layers,
//! blocks, styles, and settings — never separate undo stacks (§19.2).

use cad_core::EntityId;

use crate::layer::LayerChange;
use crate::linetype::LinetypeChange;
use crate::text_style::TextStyleChange;
use crate::{EntityRecord, SettingsChange};

/// A reversible change to one entity: to its geometry, its properties, or
/// both. Payloads are shared, not copied.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityChange {
    Insert {
        id: EntityId,
        record: EntityRecord,
    },
    Remove {
        id: EntityId,
        record: EntityRecord,
    },
    Replace {
        id: EntityId,
        before: EntityRecord,
        after: EntityRecord,
    },
}

impl EntityChange {
    pub fn id(&self) -> EntityId {
        match self {
            EntityChange::Insert { id, .. }
            | EntityChange::Remove { id, .. }
            | EntityChange::Replace { id, .. } => *id,
        }
    }

    /// The change that exactly undoes this one.
    pub fn inverse(&self) -> Self {
        match self.clone() {
            EntityChange::Insert { id, record } => EntityChange::Remove { id, record },
            EntityChange::Remove { id, record } => EntityChange::Insert { id, record },
            EntityChange::Replace { id, before, after } => EntityChange::Replace {
                id,
                before: after,
                after: before,
            },
        }
    }
}

/// One reversible change to the saved drawing: to an entity, or to a row of
/// one of its tables. One enum and one history for all of them (§19.2).
///
/// Non-exhaustive: text style and block changes join it as their tables
/// arrive.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Change {
    Entity(EntityChange),
    Layer(LayerChange),
    Linetype(LinetypeChange),
    TextStyle(TextStyleChange),
    Settings(SettingsChange),
}

impl Change {
    pub fn inverse(&self) -> Self {
        match self {
            Change::Entity(change) => Change::Entity(change.inverse()),
            Change::Layer(change) => Change::Layer(change.inverse()),
            Change::Linetype(change) => Change::Linetype(change.inverse()),
            Change::TextStyle(change) => Change::TextStyle(change.inverse()),
            Change::Settings(change) => Change::Settings(change.inverse()),
        }
    }
}

/// An atomic group of changes: one committed gesture, one undo step (§19.4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transaction {
    changes: Vec<Change>,
}

impl Transaction {
    pub(crate) fn new(changes: Vec<Change>) -> Self {
        Self { changes }
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// One transaction with the changes of all of `parts`, in order.
    pub(crate) fn merged(parts: impl IntoIterator<Item = Transaction>) -> Self {
        Self::new(parts.into_iter().flat_map(|part| part.changes).collect())
    }

    /// The transaction that exactly undoes this one: inverse changes in
    /// reverse order.
    pub fn inverse(&self) -> Self {
        Self::new(self.changes.iter().rev().map(Change::inverse).collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cad_core::Point2;

    use super::*;
    use crate::layer::{Layer, LayerId};
    use crate::linetype::{LinetypeDef, LinetypeId};
    use crate::table::TableChange;
    use crate::{DrawingSettings, LineEntity, Properties};

    fn line(x: f64) -> EntityRecord {
        EntityRecord::new(
            LineEntity::new(Point2::ORIGIN, Point2::new(x, 0.0)),
            Properties::default(),
        )
    }

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    #[test]
    fn insert_and_remove_are_inverses() {
        let insert = EntityChange::Insert {
            id: id(1),
            record: line(1.0),
        };
        let remove = EntityChange::Remove {
            id: id(1),
            record: line(1.0),
        };
        assert_eq!(insert.inverse(), remove);
        assert_eq!(remove.inverse(), insert);
    }

    #[test]
    fn replace_inverse_swaps_before_and_after() {
        let replace = EntityChange::Replace {
            id: id(1),
            before: line(1.0),
            after: line(2.0),
        };
        let expected = EntityChange::Replace {
            id: id(1),
            before: line(2.0),
            after: line(1.0),
        };
        assert_eq!(replace.inverse(), expected);
    }

    fn mixed() -> Transaction {
        Transaction::new(vec![
            Change::Layer(TableChange::Insert {
                id: LayerId::from_raw(1),
                row: Layer::named("walls"),
            }),
            Change::Entity(EntityChange::Insert {
                id: id(1),
                record: line(1.0),
            }),
            Change::Entity(EntityChange::Replace {
                id: id(2),
                before: line(2.0),
                after: line(3.0),
            }),
            Change::Layer(TableChange::Replace {
                id: LayerId::from_raw(1),
                before: Layer::named("walls"),
                after: Layer::named("doors"),
            }),
            Change::Entity(EntityChange::Remove {
                id: id(4),
                record: line(4.0),
            }),
            Change::Linetype(TableChange::Insert {
                id: LinetypeId::from_raw(1),
                row: LinetypeDef::built_in_named("Dashed").unwrap(),
            }),
            Change::Settings(SettingsChange {
                before: DrawingSettings::default(),
                after: DrawingSettings {
                    current_layer: LayerId::from_raw(1),
                    ..DrawingSettings::default()
                },
            }),
        ])
    }

    #[test]
    fn inverse_of_inverse_is_the_original_transaction() {
        let tx = mixed();
        assert_eq!(tx.inverse().inverse(), tx);
    }

    /// Entity and table changes share one order: undoing a transaction
    /// undoes them last to first, whichever kind each is.
    #[test]
    fn inverse_reverses_the_order_of_changes_of_every_kind() {
        let inverse = mixed().inverse();
        let kinds: Vec<&str> = inverse
            .changes()
            .iter()
            .map(|change| match change {
                Change::Entity(EntityChange::Insert { .. }) => "insert entity",
                Change::Entity(EntityChange::Remove { .. }) => "remove entity",
                Change::Entity(EntityChange::Replace { .. }) => "replace entity",
                Change::Layer(TableChange::Insert { .. }) => "insert layer",
                Change::Layer(TableChange::Remove { .. }) => "remove layer",
                Change::Layer(TableChange::Replace { .. }) => "replace layer",
                Change::Linetype(TableChange::Insert { .. }) => "insert linetype",
                Change::Linetype(TableChange::Remove { .. }) => "remove linetype",
                Change::Linetype(TableChange::Replace { .. }) => "replace linetype",
                Change::TextStyle(_) => "text style",
                Change::Settings(_) => "settings",
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "settings",
                "remove linetype",
                "insert entity",
                "replace layer",
                "replace entity",
                "remove entity",
                "remove layer"
            ]
        );
    }

    #[test]
    fn history_shares_payloads_instead_of_copying_them() {
        let payload = line(1.0);
        let change = EntityChange::Insert {
            id: id(1),
            record: payload.clone(),
        };
        let EntityChange::Remove { record, .. } = change.inverse() else {
            panic!("inverse of insert must be remove");
        };
        assert!(Arc::ptr_eq(&payload.entity, &record.entity));
    }
}
