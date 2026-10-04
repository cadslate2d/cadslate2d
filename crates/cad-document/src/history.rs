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
    /// Length of the undo stack when the open group began, if one is open.
    group_start: Option<usize>,
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

    /// Starts a group: everything recorded until [`History::end_group`]
    /// becomes a single undo step (one gesture, one transaction — §19.4).
    pub(crate) fn begin_group(&mut self) {
        self.group_start = Some(self.undo.len());
    }

    /// Closes the open group, merging what was recorded inside it.
    pub(crate) fn end_group(&mut self) {
        let Some(start) = self.group_start.take() else {
            return;
        };
        // An undo inside the group may have popped below where it began.
        let start = start.min(self.undo.len());
        if self.undo.len() - start > 1 {
            let merged = Transaction::merged(self.undo.drain(start..));
            self.undo.push(merged);
        }
    }

    /// Removes and returns the latest transaction recorded inside the open
    /// group, without making it redoable. `None` when the group is empty or
    /// no group is open.
    pub(crate) fn take_last_in_group(&mut self) -> Option<Transaction> {
        let start = self.group_start?;
        if self.undo.len() > start {
            self.undo.pop()
        } else {
            None
        }
    }
}
