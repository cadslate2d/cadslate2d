//! The command abstraction (DESIGN §15).
//!
//! Every command is an explicit state machine (State pattern). It consumes
//! typed and pointer input through the same [`Command::step`], so a point is a
//! point no matter where it came from, and a toolbar button can never bypass
//! the command architecture.

use std::fmt::Debug;

use cad_core::Point2;
use cad_document::Drawing;
use cad_input::ExpectedInput;

use crate::log::CommandLog;
use crate::{HostRequest, OverlayItem, Selection};

/// One resolved input handed to the active command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CommandInput {
    /// A world point — already resolved from typed coordinates or the pointer.
    Point(Point2),
    /// Enter or Space on an empty line.
    Accept,
}

/// What a command wants after consuming an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Keep the command active and prompt again.
    Continue,
    /// The command is complete; return to the idle prompt.
    Finished,
}

/// What a command may touch while it runs.
///
/// The drawing is mutable only through its transaction API, so a command
/// cannot change the document without producing an undo step.
#[derive(Debug)]
pub struct CommandContext<'a> {
    pub drawing: &'a mut Drawing,
    pub selection: &'a mut Selection,
    pub(crate) log: &'a mut CommandLog,
    pub(crate) host_request: &'a mut Option<HostRequest>,
}

impl CommandContext<'_> {
    /// Writes a line to the command log.
    pub fn message(&mut self, text: impl Into<String>) {
        self.log.push(text.into());
    }

    /// Asks the host to do something a command cannot: show a file picker,
    /// offer a download. The host picks the request up after this step.
    pub fn request_host(&mut self, request: HostRequest) {
        *self.host_request = Some(request);
    }
}

/// An interactive command.
pub trait Command: Debug {
    /// Called once when the command is invoked. Commands that need no input
    /// do their work here and return [`Step::Finished`].
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    /// The prompt for the current state, for example `Specify next point:`.
    fn prompt(&self) -> String;

    /// What the current state accepts. Drives input resolution (§13.3).
    fn expected(&self) -> ExpectedInput;

    /// Consumes one input and advances the state machine.
    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step;

    /// Describes, semantically, what the command would create if the next
    /// point were `cursor`. The preview is transient: it lives in the overlay
    /// and never touches the document (§17).
    fn preview(&self, _cursor: Point2, _out: &mut Vec<OverlayItem>) {}
}
