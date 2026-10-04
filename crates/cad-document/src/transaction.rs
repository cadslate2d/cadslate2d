//! Reversible changes to the saved drawing (DESIGN §19).
//!
//! Every persisted mutation is a [`Change`] inside a [`Transaction`]. There is
//! one change model and one history for entities and, later, for layers,
//! blocks, styles, and settings — never separate undo stacks (§19.2).

use cad_core::EntityId;

use crate::SharedEntity;

/// A reversible change to one entity. Payloads are shared, not copied.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityChange {
    Insert {
        id: EntityId,
        entity: SharedEntity,
    },
    Remove {
        id: EntityId,
        entity: SharedEntity,
    },
    Replace {
        id: EntityId,
        before: SharedEntity,
        after: SharedEntity,
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
            EntityChange::Insert { id, entity } => EntityChange::Remove { id, entity },
            EntityChange::Remove { id, entity } => EntityChange::Insert { id, entity },
            EntityChange::Replace { id, before, after } => EntityChange::Replace {
                id,
                before: after,
                after: before,
            },
        }
    }
}

/// One reversible change to the saved drawing.
///
/// Non-exhaustive: layer, block, style, and settings changes join this enum in
/// later milestones and share the same history.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Change {
    Entity(EntityChange),
}

impl Change {
    pub fn inverse(&self) -> Self {
        match self {
            Change::Entity(change) => Change::Entity(change.inverse()),
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
    use crate::{Entity, LineEntity};

    fn line(x: f64) -> SharedEntity {
        Arc::new(Entity::Line(LineEntity::new(
            Point2::ORIGIN,
            Point2::new(x, 0.0),
        )))
    }

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    #[test]
    fn insert_and_remove_are_inverses() {
        let insert = EntityChange::Insert {
            id: id(1),
            entity: line(1.0),
        };
        let remove = EntityChange::Remove {
            id: id(1),
            entity: line(1.0),
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

    #[test]
    fn inverse_of_inverse_is_the_original_transaction() {
        let tx = Transaction::new(vec![
            Change::Entity(EntityChange::Insert {
                id: id(1),
                entity: line(1.0),
            }),
            Change::Entity(EntityChange::Replace {
                id: id(2),
                before: line(2.0),
                after: line(3.0),
            }),
            Change::Entity(EntityChange::Remove {
                id: id(4),
                entity: line(4.0),
            }),
        ]);
        assert_eq!(tx.inverse().inverse(), tx);
    }

    #[test]
    fn inverse_reverses_the_order_of_changes() {
        let tx = Transaction::new(vec![
            Change::Entity(EntityChange::Insert {
                id: id(1),
                entity: line(1.0),
            }),
            Change::Entity(EntityChange::Insert {
                id: id(2),
                entity: line(2.0),
            }),
        ]);
        let ids: Vec<EntityId> = tx
            .inverse()
            .changes()
            .iter()
            .map(|Change::Entity(change)| change.id())
            .collect();
        assert_eq!(ids, vec![id(2), id(1)]);
    }

    #[test]
    fn history_shares_payloads_instead_of_copying_them() {
        let payload = line(1.0);
        let change = EntityChange::Insert {
            id: id(1),
            entity: Arc::clone(&payload),
        };
        let EntityChange::Remove { entity, .. } = change.inverse() else {
            panic!("inverse of insert must be remove");
        };
        assert!(Arc::ptr_eq(&payload, &entity));
    }
}
