//! Linear undo/redo history (DESIGN §19.3, LOCKED).

use crate::Transaction;

/// Two stacks of committed transactions. Recording new work after an undo
/// discards the redo branch.
///
/// History holds transactions only — never spatial-index or render-cache
/// state, which is derived and rebuilt from the document (§19.5).
#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl History {
    pub(crate) fn record(&mut self, transaction: Transaction) {
        self.undo.push(transaction);
        self.redo.clear();
    }

    /// Moves the latest transaction to the redo stack and returns it so the
    /// caller can apply its inverse.
    pub(crate) fn step_back(&mut self) -> Option<&Transaction> {
        let transaction = self.undo.pop()?;
        self.redo.push(transaction);
        self.redo.last()
    }

    /// Moves the latest undone transaction back and returns it so the caller
    /// can re-apply it.
    pub(crate) fn step_forward(&mut self) -> Option<&Transaction> {
        let transaction = self.redo.pop()?;
        self.undo.push(transaction);
        self.undo.last()
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
