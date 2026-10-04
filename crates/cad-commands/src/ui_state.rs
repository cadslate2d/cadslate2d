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
    /// The world point a click would give, when the pointer is over the canvas.
    pub cursor: Option<(f64, f64)>,
    /// The object snap the cursor is on, by name; `cursor` is then the exact
    /// snap point — what a click would give.
    pub snap: Option<&'static str>,
    /// Number of selected entities.
    pub selection_count: usize,
    /// Whether Space submits the command line like Enter. False only while a
    /// command expects free text (none do yet).
    pub space_accepts: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Drafting toggles that are on, one bit each (`Toggle::bit`).
    pub toggles: u32,
    /// Name of the file the drawing was opened from, if any.
    pub document_name: Option<String>,
    /// The drawing changed since it was opened or last exported.
    pub unsaved_changes: bool,
    /// The drawing was opened from a file with content that could not be
    /// imported, so exporting it will not reproduce that file. The UI must
    /// warn before exporting (DESIGN §24.2).
    pub export_is_lossy: bool,
}
