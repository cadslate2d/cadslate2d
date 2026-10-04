//! Stable logical entity identity (DESIGN §10.1, LOCKED).

use std::fmt;

/// The stable semantic identity of an entity.
///
/// IDs are allocated monotonically by the drawing and never reused, even after
/// deletion or undo. Physical storage location is a separate, mutable concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(u64);

impl EntityId {
    /// Rebuilds an ID from its raw value.
    ///
    /// This is the explicit restore path used by the drawing's allocator and by
    /// undo. Other code receives IDs from the drawing and must not invent them.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_value_round_trips() {
        assert_eq!(EntityId::from_raw(734).raw(), 734);
    }

    #[test]
    fn ids_order_by_allocation() {
        assert!(EntityId::from_raw(10) < EntityId::from_raw(11));
    }

    #[test]
    fn display_is_compact() {
        assert_eq!(EntityId::from_raw(42).to_string(), "#42");
    }
}
