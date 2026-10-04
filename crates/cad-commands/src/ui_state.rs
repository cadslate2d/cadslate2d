//! What the UI shell needs to present the session (ADR-0002 §5).

/// A small snapshot of presentation state. It deliberately contains nothing
/// from which drawing geometry could be reconstructed.
#[derive(Clone, Debug, PartialEq)]
pub struct UiState {
    /// The current prompt, for example `Command:` or `Specify next point:`.
    pub prompt: String,
    /// Canonical name of the active command, if any.
    pub command: Option<&'static str>,
    /// Total log lines ever written; fetch new ones with `Session::log_since`.
    pub log_total: u64,
    /// World position under the cursor, when the pointer is over the canvas.
    pub cursor: Option<(f64, f64)>,
    /// Number of selected entities.
    pub selection_count: usize,
    /// Whether Space submits the command line like Enter. False only while a
    /// command expects free text (none do yet).
    pub space_accepts: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}
