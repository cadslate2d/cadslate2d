//! Monotonic entity ID allocation (DESIGN §10.2, LOCKED).

use cad_core::EntityId;

/// Hands out entity IDs in increasing order for the lifetime of a drawing.
///
/// There is deliberately no way to lower or reset the counter: undo removes
/// entities but never returns their IDs to the pool.
#[derive(Debug)]
pub(crate) struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    pub(crate) fn new() -> Self {
        Self { next: 1 }
    }

    pub(crate) fn allocate(&mut self) -> EntityId {
        let id = EntityId::from_raw(self.next);
        self.next = self.next.checked_add(1).expect("entity ID space exhausted");
        id
    }

    /// The raw value the next allocation will return.
    pub(crate) fn peek(&self) -> u64 {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_strictly_increasing() {
        let mut ids = EntityIdAllocator::new();
        let a = ids.allocate();
        let b = ids.allocate();
        let c = ids.allocate();
        assert!(a < b && b < c);
    }

    #[test]
    fn peek_reports_the_next_id_without_consuming_it() {
        let mut ids = EntityIdAllocator::new();
        let next = ids.peek();
        assert_eq!(ids.peek(), next);
        assert_eq!(ids.allocate().raw(), next);
        assert_eq!(ids.peek(), next + 1);
    }
}
