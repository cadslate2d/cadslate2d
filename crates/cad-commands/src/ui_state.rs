//! What the UI shell needs to present the session (ADR-0002 §5).

use crate::{FileFormat, SelectionSummary};

/// What an import did and did not bring in, as lines for the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    /// Counts the imports of the session, so the host can tell a new
    /// report from one it has shown.
    pub serial: u32,
    pub lines: Vec<String>,
}

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
    /// The readout of ORTHO or POLAR while it constrains the pointer, such as
    /// `Polar: 50.0000 < 45°`.
    pub tracking: Option<String>,
    /// Number of selected entities.
    pub selection_count: usize,
    /// Whether Space submits the command line like Enter. False only while a
    /// command expects free text (none do yet).
    pub space_accepts: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Whether the session's clipboard holds something to paste (#247):
    /// Edit › Paste is greyed out otherwise.
    pub can_paste: bool,
    /// Whether constraint bars, and constraint dimensions, are shown
    /// (#269): View › Constraint Bars checks them.
    pub constraint_bars: bool,
    pub constraint_dimensions: bool,
    /// Drafting toggles that are on, one bit each (`Toggle::bit`).
    pub toggles: u32,
    /// The drawing's unit as its suffix — `mm`, `in` — or none (#186).
    pub unit: Option<&'static str>,
    /// The current isometric plane while the snap style is isometric
    /// (#336): `Left`, `Top`, `Right`.
    pub isoplane: Option<&'static str>,
    /// The user's command aliases, one per line, for the host to keep (#186).
    pub user_aliases: String,
    /// Name of the file the drawing was opened from, if any.
    pub document_name: Option<String>,
    /// The format that file came in (#318); DXF for a new drawing.
    pub document_format: FileFormat,
    /// The drawing changed since it was opened or last exported.
    pub unsaved_changes: bool,
    /// The drawing's revision: changes on every commit, undo, and redo, so
    /// the host knows when there is something new to keep (autosave).
    pub revision: u64,
    /// The drawing was opened from a file with content that could not be
    /// imported, so exporting it will not reproduce that file. The UI must
    /// warn before exporting (DESIGN §24.2).
    /// What an export will not include, line by line: content the file the
    /// drawing came from held and the drawing could not. Empty when an
    /// export reproduces the drawing whole; the host warns before exporting
    /// otherwise (DESIGN §24.2).
    pub export_losses: Vec<String>,
    /// The layouts, by name, in tab order (#193).
    pub layouts: Vec<String>,
    /// The name of the space the session is in — `Model`, or a layout's
    /// — the layout's while the model is worked through its viewport.
    pub space: String,
    /// Whether the session is on a layout's paper.
    pub paper: bool,
    /// Whether the model is worked through a viewport (`MSPACE`).
    pub through: bool,
    /// The account of the last import, for the host to show until it is
    /// dismissed or another drawing is opened.
    pub import_report: Option<ImportReport>,
    /// The layers, in the order they were made, as the Layers panel shows
    /// them. Presentation only: names and looks, no IDs and no geometry.
    pub layers: Vec<LayerRow>,
    /// The names of the loaded linetypes, in the order they were loaded:
    /// what a linetype field can offer.
    pub linetypes: Vec<String>,
    /// The selection as the Properties panel shows it: counts and shared
    /// values, bounded whatever the selection's size.
    pub selection: SelectionSummary,
    /// The text to open the paragraph editor with, while the active
    /// command waits for a paragraph (MTEXT, TEXTEDIT on one).
    pub text_editor: Option<String>,
    /// Whether that editor takes one line, Enter finishing it: a text's
    /// words rather than a paragraph.
    pub text_editor_line: bool,
}

/// One row of the Layers panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerRow {
    pub name: String,
    /// The layer's colour number, 1 to 255.
    pub color: u8,
    /// What the colour looks like, or `None` for the theme's foreground.
    pub rgb: Option<[u8; 3]>,
    pub on: bool,
    /// Frozen (#213): hidden like a layer that is off, switched apart.
    pub frozen: bool,
    pub locked: bool,
    /// New objects go on this layer.
    pub current: bool,
    /// The linetype's name.
    pub linetype: String,
    /// The lineweight as it is shown: `Default`, `0.25 mm`.
    pub lineweight: String,
}
