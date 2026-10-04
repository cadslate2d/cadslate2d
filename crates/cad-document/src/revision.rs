//! Document revisions.

use std::sync::atomic::{AtomicU64, Ordering};

/// Identifies one state of one drawing.
///
/// A drawing takes a fresh revision whenever its canonical content changes
/// (commit, undo, redo) and at no other time. Revisions are unique within the
/// process, so a cache keyed by revision stays correct even when one drawing
/// is replaced by another, for example on file open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Revision(u64);

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Revision {
    pub(crate) fn next() -> Self {
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_revision_is_distinct() {
        let a = Revision::next();
        let b = Revision::next();
        assert_ne!(a, b);
        assert!(b.as_u64() > a.as_u64());
    }
}
