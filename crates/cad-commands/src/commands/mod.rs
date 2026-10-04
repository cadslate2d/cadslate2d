//! The built-in commands. Each is an explicit state machine (DESIGN §15).

mod history;
mod line;

pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use line::LineCommand;
