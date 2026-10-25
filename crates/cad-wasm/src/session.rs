//! The one exported handle (ADR-0002 §1).

use cad_commands::{
    Dirty, FileFormat, HostRequest, KeyAction, LayerRow, Modifiers, PointerButton,
    SelectionSummary, Session, Toggle, Toggles, UiState,
};
use cad_core::{ClipTransform, LengthUnit, Point2, ScreenPoint};
use cad_document::{Entity, LayoutId, ViewportEntity};
use cad_render::{OverlayFrame, RenderScene, SceneCache, viewport_view};
use js_sys::{Float32Array, Uint8Array, Uint32Array};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::views::{f32_view, u8_view, u32_view};

const MODIFIER_SHIFT: u32 = 1;
const MODIFIER_CTRL: u32 = 1 << 1;
const MODIFIER_ALT: u32 = 1 << 2;

fn modifiers(bits: u32) -> Modifiers {
    Modifiers {
        shift: bits & MODIFIER_SHIFT != 0,
        ctrl: bits & MODIFIER_CTRL != 0,
        alt: bits & MODIFIER_ALT != 0,
    }
}

/// DOM `MouseEvent.button` numbering. Unknown buttons are ignored.
fn button(code: u32) -> Option<PointerButton> {
    match code {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Middle),
        2 => Some(PointerButton::Secondary),
        _ => None,
    }
}

/// Key action codes, mirrored by `KeyAction` in the TypeScript wrapper.
fn key_action(code: u32) -> Option<KeyAction> {
    match code {
        0 => Some(KeyAction::Escape),
        1 => Some(KeyAction::Undo),
        2 => Some(KeyAction::Redo),
        3 => Some(KeyAction::Open),
        4 => Some(KeyAction::Save),
        5 => Some(KeyAction::Delete),
        6 => Some(KeyAction::Toggle(Toggle::Osnap)),
        7 => Some(KeyAction::Toggle(Toggle::Grid)),
        8 => Some(KeyAction::Toggle(Toggle::Ortho)),
        9 => Some(KeyAction::Toggle(Toggle::Polar)),
        10 => Some(KeyAction::Toggle(Toggle::Lineweight)),
        11 => Some(KeyAction::GroupSelection),
        12 => Some(KeyAction::Toggle(Toggle::Snap)),
        13 => Some(KeyAction::Toggle(Toggle::Otrack)),
        14 => Some(KeyAction::Copy),
        15 => Some(KeyAction::Cut),
        16 => Some(KeyAction::Paste),
        _ => None,
    }
}

fn transform_words(transform: ClipTransform) -> Vec<f32> {
    vec![
        transform.scale[0],
        transform.scale[1],
        transform.translate[0],
        transform.translate[1],
    ]
}

/// The DWG version a format names; not for DXF.
fn dwg_version(format: FileFormat) -> cad_dwg::Version {
    match format {
        FileFormat::Dxf | FileFormat::Dwg2000 => cad_dwg::Version::R2000,
        FileFormat::Dwg2004 => cad_dwg::Version::R2004,
        FileFormat::Dwg2007 => cad_dwg::Version::R2007,
    }
}

/// Reads a drawing file by its signature (#318): a DWG of a version the
/// crate reads, a DWG of another version refused by name, a DXF otherwise.
fn open_drawing_file(bytes: &[u8]) -> Result<(cad_dxf::Import, FileFormat), String> {
    match cad_dwg::Version::detect(bytes) {
        Ok(version) => {
            let import = cad_dwg::import(bytes).map_err(|e| e.to_string())?;
            let format = match version {
                cad_dwg::Version::R2000 => FileFormat::Dwg2000,
                cad_dwg::Version::R2004 => FileFormat::Dwg2004,
                cad_dwg::Version::R2007 => FileFormat::Dwg2007,
            };
            Ok((import, format))
        }
        Err(cad_dwg::DwgError::NotDwg) => cad_dxf::import(bytes)
            .map(|import| (import, FileFormat::Dxf))
            .map_err(|e| e.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// What a request to the AI may carry (#260).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiContextDto {
    unit: Option<String>,
    layers: Vec<String>,
    objects: Vec<AiObjectDto>,
    parameters: Vec<String>,
    left_out: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiObjectDto {
    /// An entity ID: exact in a double up to 2^53.
    id: f64,
    kind: &'static str,
    geometry: String,
    layer: Option<String>,
}

/// A proposal as the panel shows it (#254).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalSummaryDto {
    headline: String,
    added: u32,
    changed: u32,
    removed: u32,
    operations: Vec<String>,
    error: Option<String>,
}

/// `UiState` as the plain object JavaScript receives.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiStateDto {
    prompt: String,
    command: Option<&'static str>,
    log_total: f64,
    cursor: Option<(f64, f64)>,
    snap: Option<&'static str>,
    tracking: Option<String>,
    selection_count: u32,
    space_accepts: bool,
    can_undo: bool,
    can_redo: bool,
    can_paste: bool,
    constraint_bars: bool,
    constraint_dimensions: bool,
    toggles: u32,
    /// The drawing's unit suffix, or empty for unitless (#186).
    unit: String,
    /// The user's command aliases, one per line (#186).
    user_aliases: String,
    document_name: Option<String>,
    /// The format the file came in (#318): `dxf`, `dwg2000`, `dwg2004`,
    /// or `dwg2007`.
    document_format: &'static str,
    unsaved_changes: bool,
    revision: f64,
    export_losses: Vec<String>,
    layouts: Vec<String>,
    /// The space the session is in, by name; whether it is paper; whether
    /// the model is worked through a viewport (#193).
    space: String,
    paper: bool,
    through: bool,
    import_report: Option<ImportReportDto>,
    layers: Vec<LayerRowDto>,
    linetypes: Vec<String>,
    selection: SelectionSummaryDto,
    text_editor: Option<String>,
}

/// The last import's account: lines for the host to show (#157).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReportDto {
    serial: u32,
    lines: Vec<String>,
}

/// The selection as the Properties panel shows it: bounded whatever the
/// selection's size (#141).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionSummaryDto {
    count: u32,
    headline: String,
    /// `[kind, count]` pairs, at most one per kind of entity.
    kinds: Vec<(String, u32)>,
    layer: Option<String>,
    color: Option<String>,
    linetype: Option<String>,
    lineweight: Option<String>,
    /// `[label, value]` pairs: one object's defining geometry.
    geometry: Vec<(String, String)>,
    /// How the objects were selected, or empty (#185).
    method: String,
    /// The groups the selected objects are in, by name (#214).
    groups: Vec<String>,
}

impl From<SelectionSummary> for SelectionSummaryDto {
    fn from(summary: SelectionSummary) -> Self {
        Self {
            count: summary.count as u32,
            headline: summary.headline(),
            kinds: summary
                .kinds
                .iter()
                .map(|(kind, count)| (kind.clone(), *count as u32))
                .collect(),
            layer: summary.layer,
            color: summary.color,
            linetype: summary.linetype,
            lineweight: summary.lineweight,
            geometry: summary.geometry,
            method: summary.method,
            groups: summary.groups,
        }
    }
}

/// One row of the Layers panel: presentation data only (ADR-0002 §5).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LayerRowDto {
    name: String,
    color: u8,
    rgb: Option<[u8; 3]>,
    on: bool,
    frozen: bool,
    locked: bool,
    current: bool,
    linetype: String,
    lineweight: String,
}

impl From<LayerRow> for LayerRowDto {
    fn from(row: LayerRow) -> Self {
        Self {
            name: row.name,
            color: row.color,
            rgb: row.rgb,
            on: row.on,
            frozen: row.frozen,
            locked: row.locked,
            current: row.current,
            linetype: row.linetype,
            lineweight: row.lineweight,
        }
    }
}

impl From<UiState> for UiStateDto {
    fn from(state: UiState) -> Self {
        Self {
            prompt: state.prompt,
            command: state.command,
            log_total: state.log_total as f64,
            cursor: state.cursor,
            snap: state.snap,
            tracking: state.tracking,
            selection_count: state.selection_count as u32,
            space_accepts: state.space_accepts,
            can_undo: state.can_undo,
            can_redo: state.can_redo,
            can_paste: state.can_paste,
            constraint_bars: state.constraint_bars,
            constraint_dimensions: state.constraint_dimensions,
            toggles: state.toggles,
            document_name: state.document_name,
            document_format: match state.document_format {
                FileFormat::Dxf => "dxf",
                FileFormat::Dwg2000 => "dwg2000",
                FileFormat::Dwg2004 => "dwg2004",
                FileFormat::Dwg2007 => "dwg2007",
            },
            unsaved_changes: state.unsaved_changes,
            revision: state.revision as f64,
            export_losses: state.export_losses,
            layouts: state.layouts,
            space: state.space,
            paper: state.paper,
            through: state.through,
            unit: state.unit.unwrap_or("").to_owned(),
            user_aliases: state.user_aliases,
            import_report: state.import_report.map(|report| ImportReportDto {
                serial: report.serial,
                lines: report.lines,
            }),
            layers: state.layers.into_iter().map(LayerRowDto::from).collect(),
            linetypes: state.linetypes,
            selection: state.selection.into(),
            text_editor: state.text_editor,
        }
    }
}

/// One open drawing. JavaScript never sees the document or its entities —
/// only this handle, the dirty flags it returns, and bulk frame data.
#[wasm_bindgen]
#[derive(Debug)]
pub struct CadSession {
    session: Session,
    scene: SceneCache,
    /// An autosave snapshot under way, in slices (#178).
    snapshot: Option<cad_dxf::Export>,
    overlay: OverlayFrame,
    /// Set by any input that reports `Dirty::OVERLAY`; cleared when rebuilt.
    overlay_stale: bool,
}

impl Default for CadSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CadSession {
    /// Records what an input changed and returns the flags for JavaScript.
    fn report(&mut self, dirty: Dirty) -> u32 {
        self.overlay_stale |= dirty.contains(Dirty::OVERLAY);
        dirty.bits()
    }

    /// Rebuilds the overlay if an input has invalidated it. Idempotent, so the
    /// paired `overlay_segments` / `overlay_batches` pulls do the work once and
    /// the second one allocates nothing — which keeps the first view valid.
    fn sync_overlay(&mut self) {
        if std::mem::take(&mut self.overlay_stale) {
            let items = self.session.overlay();
            self.overlay.rebuild_in(
                &items,
                self.session.drawing(),
                self.session.space(),
                self.session.view(),
            );
        }
    }

    /// The committed scene of the space the session is in (#193).
    fn scene(&mut self) -> &RenderScene {
        self.scene
            .scene_in(self.session.drawing(), self.session.space())
    }
}

#[wasm_bindgen]
impl CadSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            session: Session::new(),
            scene: SceneCache::new(),
            snapshot: None,
            overlay: OverlayFrame::new(),
            overlay_stale: true,
        }
    }

    // ── input: one call per browser event, each returning dirty flags ──

    /// Canvas size in CSS pixels.
    pub fn resize(&mut self, width: f64, height: f64) -> u32 {
        let dirty = self.session.resize(width, height);
        self.report(dirty)
    }

    pub fn pointer_move(&mut self, x: f64, y: f64, modifier_bits: u32) -> u32 {
        let dirty = self
            .session
            .pointer_move(ScreenPoint::new(x, y), modifiers(modifier_bits));
        self.report(dirty)
    }

    /// The pointer has rested a moment without moving (#220): the host's
    /// timer says so, since Rust keeps no clock. Object snap tracking
    /// acquires or releases the snap point under it.
    pub fn pointer_rest(&mut self) -> u32 {
        let dirty = self.session.pointer_rest();
        self.report(dirty)
    }

    /// The pointer left the canvas.
    pub fn pointer_leave(&mut self) -> u32 {
        let dirty = self.session.pointer_leave();
        self.report(dirty)
    }

    pub fn pointer_down(&mut self, x: f64, y: f64, button_code: u32, modifier_bits: u32) -> u32 {
        let Some(button) = button(button_code) else {
            return 0;
        };
        let dirty =
            self.session
                .pointer_down(ScreenPoint::new(x, y), button, modifiers(modifier_bits));
        self.report(dirty)
    }

    pub fn pointer_up(&mut self, x: f64, y: f64, button_code: u32, modifier_bits: u32) -> u32 {
        let Some(button) = button(button_code) else {
            return 0;
        };
        let dirty =
            self.session
                .pointer_up(ScreenPoint::new(x, y), button, modifiers(modifier_bits));
        self.report(dirty)
    }

    /// `delta_y` in CSS pixels, as in `WheelEvent.deltaY` with pixel delta mode.
    pub fn wheel(&mut self, x: f64, y: f64, delta_y: f64, modifier_bits: u32) -> u32 {
        let dirty = self
            .session
            .wheel(ScreenPoint::new(x, y), delta_y, modifiers(modifier_bits));
        self.report(dirty)
    }

    /// A non-text key. Unknown codes are ignored.
    pub fn key(&mut self, action_code: u32) -> u32 {
        let Some(action) = key_action(action_code) else {
            return 0;
        };
        let dirty = self.session.key(action);
        self.report(dirty)
    }

    /// One completed command-line entry. Interpreted entirely in Rust.
    pub fn submit_text(&mut self, text: &str) -> u32 {
        let dirty = self.session.submit_text(text);
        self.report(dirty)
    }

    /// Starts a command from a toolbar button or menu item. It goes through
    /// the same entry point as a typed command and is echoed like one.
    pub fn invoke(&mut self, name: &str) -> u32 {
        let dirty = self.session.invoke_from_ui(name);
        self.report(dirty)
    }

    /// Recalls an older submitted line; `undefined` when there is none.
    pub fn history_previous(&mut self) -> Option<String> {
        self.session.history_previous()
    }

    /// Recalls a newer submitted line; an empty string means "a new line".
    pub fn history_next(&mut self) -> Option<String> {
        self.session.history_next()
    }

    // ── files: the host does the I/O, Rust does the format ──

    /// The request behind a `HOST` dirty flag: `0` none, `1` open a file,
    /// `2` save the drawing to a file the user chooses, `3` save it back to
    /// its file, `4` show the settings panel, `5` plot to a PDF. Taking it
    /// clears it.
    pub fn take_host_request(&mut self) -> u32 {
        match self.session.take_host_request() {
            None => 0,
            Some(HostRequest::OpenFile) => 1,
            // The format the export asked for (#319) rides in the high
            // byte: its index in `FileFormat::ALL`, one-based.
            Some(HostRequest::ExportFile(format)) => {
                let index = FileFormat::ALL
                    .iter()
                    .position(|f| *f == format)
                    .unwrap_or(0);
                2 | ((index as u32 + 1) << 8)
            }
            Some(HostRequest::SaveFile) => 3,
            Some(HostRequest::Settings) => 4,
            Some(HostRequest::Plot) => 5,
            Some(HostRequest::InsertFile) => 6,
            Some(HostRequest::WriteBlock) => 7,
            Some(HostRequest::NewDrawing) => 8,
        }
    }

    /// The space the session is in as a one-page PDF with the session's
    /// plot settings (#187, #194): the model, or a layout's sheet with the
    /// model through its viewports. ASCII, so it travels as text. The
    /// host offers it for download under the drawing's name.
    pub fn plot_pdf(&self) -> String {
        cad_plot::plot_pdf_in(
            self.session.drawing(),
            self.session.space(),
            &self.session.settings().plot,
        )
    }

    /// What the browser kept between sessions, given back once on start
    /// (#186): the toggles' bits, the unit's suffix (empty for none), and
    /// the user's aliases as text. One coarse call (ADR-0002).
    pub fn restore_settings(&mut self, toggles: u32, unit: &str, aliases: &str) -> u32 {
        let unit = LengthUnit::from_suffix(unit);
        let dirty = self
            .session
            .restore_settings(Toggles::from_bits(toggles), unit, aliases);
        self.report(dirty)
    }

    /// Replaces the drawing with an empty one (#245): what `NEW` asked
    /// for, once the host has asked about unsaved work.
    pub fn new_drawing(&mut self) -> u32 {
        let dirty = self.session.new_drawing();
        self.report(dirty)
    }

    /// Replaces the drawing with the contents of a drawing file: a DWG by
    /// its first six bytes (#318), a DXF otherwise. On failure the current
    /// drawing is kept and the reason is written to the command log.
    pub fn load_drawing(&mut self, bytes: &[u8], name: &str) -> u32 {
        let dirty = match open_drawing_file(bytes) {
            Ok((import, format)) => self.session.open_drawing(
                import.drawing,
                name,
                format,
                import.report.summary(),
                import.report.losses(),
            ),
            Err(error) => self.session.notify(format!("Cannot open {name}: {error}.")),
        };
        self.report(dirty)
    }

    /// The file `INSERT ~` asked for (#222), read and handed to the
    /// command as a drawing; what the file held that the drawing cannot
    /// goes to the log. A file that is not a drawing is said so.
    pub fn insert_dxf(&mut self, bytes: &[u8], name: &str) -> u32 {
        let dirty = match cad_dxf::import(bytes) {
            Ok(import) => {
                let messages = import.report.unsupported_line().into_iter().collect();
                self.session.insert_drawing(import.drawing, name, messages)
            }
            Err(error) => self
                .session
                .notify(format!("Cannot insert {name}: {error}.")),
        };
        self.report(dirty)
    }

    /// The file `WBLOCK` made (#222), once: its name and its text, as two
    /// strings, or none.
    pub fn take_block_file(&mut self) -> Vec<String> {
        self.session
            .take_block_file()
            .map(|(name, drawing)| vec![name, cad_dxf::export(&drawing)])
            .unwrap_or_default()
    }

    /// Runs an intent document — the JSON a model wrote (ADR-0037, #254) —
    /// against a copy of the drawing and keeps it as the proposal, which
    /// the overlay then shows. The drawing is untouched. A document that
    /// is not valid is refused in the log, and no proposal is kept.
    pub fn propose(&mut self, intent: &str) -> u32 {
        let dirty = match cad_intent::parse(intent) {
            Ok(intent) => self.session.propose(intent),
            Err(error) => {
                self.session.reject_proposal()
                    | self
                        .session
                        .notify(format!("The proposal is not valid: {error}."))
            }
        };
        self.report(dirty)
    }

    /// Applies the proposal as one undo step (#254).
    pub fn accept_proposal(&mut self) -> u32 {
        let dirty = self.session.accept_proposal();
        self.report(dirty)
    }

    /// Drops the proposal; the drawing was never touched (#254).
    pub fn reject_proposal(&mut self) -> u32 {
        let dirty = self.session.reject_proposal();
        self.report(dirty)
    }

    /// What a request to the AI may carry, whole, for the user to see
    /// before consenting to send it (ADR-0037, #260).
    pub fn ai_context(&self) -> Result<JsValue, JsError> {
        let context = self.session.ai_context();
        Ok(serde_wasm_bindgen::to_value(&AiContextDto {
            unit: context.unit.map(str::to_owned),
            layers: context.layers,
            objects: context
                .objects
                .into_iter()
                .map(|object| AiObjectDto {
                    id: object.id as f64,
                    kind: object.kind,
                    geometry: object.geometry,
                    layer: object.layer,
                })
                .collect(),
            parameters: context.parameters,
            left_out: context.left_out as u32,
        })?)
    }

    /// The proposal as the panel shows it, whole, or `undefined` (#254).
    pub fn proposal_summary(&self) -> Result<JsValue, JsError> {
        let summary = self.session.proposal().map(|proposal| {
            let summary = proposal.summary();
            ProposalSummaryDto {
                headline: summary.headline,
                added: summary.added as u32,
                changed: summary.changed as u32,
                removed: summary.removed as u32,
                operations: summary.operations,
                error: summary.error,
            }
        });
        Ok(serde_wasm_bindgen::to_value(&summary)?)
    }

    /// Takes the import report off the screen (#157).
    pub fn dismiss_import_report(&mut self) -> u32 {
        let dirty = self.session.dismiss_import_report();
        self.report(dirty)
    }

    /// The drawing as a file of `format` — `dxf`, `dwg2000`, `dwg2004`,
    /// `dwg2007` (#319) — for the host to save: marks nothing, so what the
    /// user has and has not exported stays true. Empty for a format this
    /// crate does not know or cannot write, which the log then says.
    pub fn snapshot_file(&mut self, format: &str) -> Vec<u8> {
        let drawing = self.session.drawing();
        match FileFormat::from_id(format) {
            Some(FileFormat::Dxf) => cad_dxf::export(drawing).into_bytes(),
            Some(format) => match cad_dwg::export(drawing, dwg_version(format)) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.session
                        .notify(format!("Cannot write {}: {error}.", format.label()));
                    Vec::new()
                }
            },
            None => Vec::new(),
        }
    }

    /// What a file of `format` will not include of the drawing (#319),
    /// line by line: the DWG writer's own losses; a DXF holds everything.
    pub fn export_losses(&self, format: &str) -> Vec<String> {
        match FileFormat::from_id(format) {
            Some(FileFormat::Dxf) | None => Vec::new(),
            Some(format) => cad_dwg::export_losses(self.session.drawing(), dwg_version(format)),
        }
    }

    /// Begins a snapshot in slices (#178, ADR-0024): the same file as
    /// `snapshot_dxf`, written a few records per `snapshot_step`, so the
    /// autosave of a large drawing shares the main thread with the frames
    /// between. Begins afresh if one was under way.
    pub fn snapshot_begin(&mut self) {
        self.snapshot = Some(cad_dxf::Export::begin(self.session.drawing()));
    }

    /// Writes up to `budget` more records of the snapshot: 0 while there
    /// is more to write, 1 when the file is complete, 2 when the drawing
    /// changed since the snapshot began — it is dropped, and the next pause
    /// begins another — or none was begun.
    pub fn snapshot_step(&mut self, budget: u32) -> u32 {
        let Some(job) = self.snapshot.as_mut() else {
            return 2;
        };
        match job.step(self.session.drawing(), budget as usize) {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(_) => {
                self.snapshot = None;
                2
            }
        }
    }

    /// The completed snapshot as UTF-8 bytes, which ends it; empty when
    /// none is complete. Owned: safe to keep.
    pub fn snapshot_bytes(&mut self) -> Uint8Array {
        let text = self
            .snapshot
            .take()
            .and_then(|job| job.finish(self.session.drawing()).ok())
            .unwrap_or_default();
        Uint8Array::from(text.as_bytes())
    }

    /// Records that the host has saved the drawing (the bytes of
    /// `snapshot_file`) as `format` — to `name`, when it has a file now
    /// (#159, #319).
    pub fn mark_exported(&mut self, name: Option<String>, format: &str) -> u32 {
        let format = FileFormat::from_id(format).unwrap_or_default();
        let dirty = self.session.mark_exported(name.as_deref(), format);
        self.report(dirty)
    }

    // ── output: pulled in bulk, only when flagged ──

    /// Presentation state for the shell: prompt, cursor readout, counters.
    pub fn ui_state(&self) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&UiStateDto::from(
            self.session.ui_state(),
        ))?)
    }

    /// Command-log lines numbered `since` and later.
    pub fn log_since(&self, since: f64) -> Vec<String> {
        self.session.log_since(since.max(0.0) as u64)
    }

    /// Identifies the committed geometry. Re-upload committed instance
    /// buffers only when this differs from the revision last uploaded. It
    /// changes when what is shown changes — not when a layer is recoloured
    /// (ADR-0014). Cheap: does not build the scene.
    pub fn scene_revision(&self) -> f64 {
        // Revisions are small sequential integers, and the changes of
        // space fewer: together they are exact in an f64 (#193).
        let revision = self.session.drawing().shown_revision().as_u64();
        (revision * (1 << 20) + self.session.space_epoch() % (1 << 20)) as f64
    }

    /// Identifies the style table. Re-upload it only when this differs from
    /// the revision last uploaded. Cheap: does not build the scene.
    pub fn scene_style_revision(&self) -> f64 {
        self.session.drawing().revision().as_u64() as f64
    }

    /// The style table: eight bytes per row — `red, green, blue, kind,
    /// weight, pattern`, and two kept for later. Kind 255 is that colour
    /// and kind 0 the theme's foreground; weight is a lineweight in
    /// hundredths of a millimetre, or 255 for the theme's ordinary width;
    /// pattern is a row of [`Self::scene_patterns`]. Row `i` is what an
    /// instance with style index `i` is drawn with. A view into WASM memory
    /// — consume before the next call.
    pub fn scene_styles(&mut self) -> Uint8Array {
        u8_view(self.scene().styles())
    }

    /// The pattern table (ADR-0015): ten floats per row — the period, the
    /// number of lengths, then dash, gap, dash, gap … in drawing units with
    /// `LTSCALE` applied, zeros after the last. Row 0 is a continuous line.
    /// Changes with the style table. A view into WASM memory — consume
    /// before the next call.
    pub fn scene_patterns(&mut self) -> Float32Array {
        f32_view(self.scene().patterns())
    }

    /// Per committed segment instance, the distance along its entity's
    /// path at its start, in drawing units: where its linetype pattern is
    /// (ADR-0015). Changes with the geometry. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_segment_distances(&mut self) -> Float32Array {
        f32_view(self.scene().frame().segment_distances())
    }

    /// Per committed arc instance, two floats: the distance along its
    /// entity's path at the arc's counter-clockwise start, and `1` or `-1`
    /// for the direction the path runs round it. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_arc_distances(&mut self) -> Float32Array {
        f32_view(self.scene().frame().arc_distances())
    }

    /// One style word per committed segment instance: the style index in
    /// the low half, the tier of the drawing order, an `i16`, in the high
    /// (ADR-0032). A view into WASM memory — consume before the next call.
    pub fn scene_segment_styles(&mut self) -> Uint32Array {
        u32_view(self.scene().segment_styles())
    }

    /// One style word per committed arc instance. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_arc_styles(&mut self) -> Uint32Array {
        u32_view(self.scene().arc_styles())
    }

    /// Committed segment instances: `x0, y0, x1, y1` per segment, relative to
    /// the scene origin. A view into WASM memory — consume before the next call.
    pub fn scene_segments(&mut self) -> Float32Array {
        f32_view(self.scene().frame().segments())
    }

    /// Committed batches: `style, first, count` per batch. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_batches(&mut self) -> Uint32Array {
        u32_view(self.scene().frame().batch_words())
    }

    /// Committed arc instances: `cx, cy, radius, start, sweep` per arc, relative
    /// to the scene origin. A view into WASM memory — consume before the next call.
    pub fn scene_arcs(&mut self) -> Float32Array {
        f32_view(self.scene().frame().arcs())
    }

    /// Committed arc batches: `style, first, count` per batch. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_arc_batches(&mut self) -> Uint32Array {
        u32_view(self.scene().frame().arc_batch_words())
    }

    /// Clip transforms for the committed scene, one per region (#188):
    /// `[scale_x, scale_y, translate_x, translate_y]` each, in the order
    /// of `scene_regions`, a copy. Changes with the view; the scene does
    /// not. A drawing of ordinary size is one region.
    pub fn scene_transform(&mut self) -> Vec<f32> {
        let view = self.session.view().clone();
        self.scene()
            .regions()
            .iter()
            .flat_map(|region| transform_words(view.clip_transform(region.origin)))
            .collect()
    }

    /// The runs of committed instances drawn under each transform of
    /// `scene_transform` (#188): `segment_first, segment_count, arc_first,
    /// arc_count, triangle_first, triangle_count` per region, in instances.
    /// Changes with the scene; read it with the scene buffers. Owned.
    pub fn scene_regions(&mut self) -> Uint32Array {
        let words: Vec<u32> = self
            .scene()
            .regions()
            .iter()
            .flat_map(|region| {
                [
                    region.first[0],
                    region.count[0],
                    region.first[1],
                    region.count[1],
                    region.first[2],
                    region.count[2],
                    u32::from(region.paper),
                ]
            })
            .collect();
        Uint32Array::from(words.as_slice())
    }

    /// The viewports of the layout shown (#193), each as its frame in
    /// clip space — `x0, y0, x1, y1` — then one clip transform per
    /// region of `scene_regions` for the model seen through it. Empty on
    /// the model, and for a viewport that is off. Changes with the view
    /// and the drawing; a copy.
    pub fn scene_viewports(&mut self) -> Vec<f32> {
        let space = self.session.space();
        if space == LayoutId::MODEL {
            return Vec::new();
        }
        let paper = self.session.view().clone();
        let windows: Vec<ViewportEntity> = self
            .session
            .drawing()
            .shown_in(space)
            .filter_map(|(_, entity)| match entity {
                Entity::Viewport(viewport) if viewport.on => Some(*viewport),
                _ => None,
            })
            .collect();
        let regions = self.scene().regions();
        let clip = paper.clip_transform(Point2::ORIGIN);
        let to_clip = |point: Point2| {
            [
                (point.x as f32) * clip.scale[0] + clip.translate[0],
                (point.y as f32) * clip.scale[1] + clip.translate[1],
            ]
        };
        let mut out = Vec::new();
        for window in windows {
            let frame = window.frame();
            let [x0, y0] = to_clip(frame.min());
            let [x1, y1] = to_clip(frame.max());
            out.extend([x0, y0, x1, y1]);
            let view = viewport_view(&paper, &window);
            for region in &regions {
                out.extend(transform_words(view.clip_transform(region.origin)));
            }
        }
        out
    }

    /// Transient segment instances (previews, highlights, crosshair). A view
    /// into WASM memory — consume before the next call.
    pub fn overlay_segments(&mut self) -> Float32Array {
        self.sync_overlay();
        f32_view(self.overlay.frame().segments())
    }

    /// Transient batches. A view into WASM memory — consume before the next call.
    pub fn overlay_batches(&mut self) -> Uint32Array {
        self.sync_overlay();
        u32_view(self.overlay.frame().batch_words())
    }

    /// Transient arc instances. A view into WASM memory — consume before the next call.
    pub fn overlay_arcs(&mut self) -> Float32Array {
        self.sync_overlay();
        f32_view(self.overlay.frame().arcs())
    }

    /// Transient arc batches. A view into WASM memory — consume before the next call.
    pub fn overlay_arc_batches(&mut self) -> Uint32Array {
        self.sync_overlay();
        u32_view(self.overlay.frame().arc_batch_words())
    }

    /// Committed triangle instances (ADR-0021): `x0, y0, x1, y1, x2, y2`
    /// per filled triangle, relative to the scene origin. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_triangles(&mut self) -> Float32Array {
        f32_view(self.scene().frame().triangles())
    }

    /// Committed triangle batches: `style, first, count` per batch. A view
    /// into WASM memory — consume before the next call.
    pub fn scene_triangle_batches(&mut self) -> Uint32Array {
        u32_view(self.scene().frame().triangle_batch_words())
    }

    /// One style word per committed triangle instance. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_triangle_styles(&mut self) -> Uint32Array {
        u32_view(self.scene().triangle_styles())
    }

    /// Which committed instances changed since this was last called
    /// (ADR-0022): `kind, first, count` per range, kind 0 segments, 1 arcs,
    /// 2 triangles, in instance units. After a whole rebuild, one range
    /// per kind over everything. Read it before the scene buffers of the
    /// same frame and upload those ranges with `bufferSubData`; or the
    /// whole buffers, when the GPU holds no scene yet. Owned: safe to keep.
    pub fn scene_dirty_ranges(&mut self) -> Uint32Array {
        self.scene();
        let words: Vec<u32> = self
            .scene
            .take_dirty()
            .into_iter()
            .flat_map(|range| [range.kind as u32, range.first, range.count])
            .collect();
        Uint32Array::from(words.as_slice())
    }

    /// Transient triangle instances: the fill of a hatch being previewed or
    /// highlighted. A view into WASM memory — consume before the next call.
    pub fn overlay_triangles(&mut self) -> Float32Array {
        self.sync_overlay();
        f32_view(self.overlay.frame().triangles())
    }

    /// Transient triangle batches. A view into WASM memory — consume before the next call.
    pub fn overlay_triangle_batches(&mut self) -> Uint32Array {
        self.sync_overlay();
        u32_view(self.overlay.frame().triangle_batch_words())
    }

    /// Clip transform for the overlay frame, a copy.
    pub fn overlay_transform(&mut self) -> Vec<f32> {
        self.sync_overlay();
        transform_words(
            self.session
                .view()
                .clip_transform(self.overlay.frame().origin()),
        )
    }
}

#[cfg(test)]
mod tests {
    //! Headless, through the same methods the host calls: what OPEN does
    //! with a file is decided by its bytes (#318).

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-data/");
        std::fs::read(format!("{path}{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn a_dwg_of_each_version_opens_by_its_bytes_whatever_its_name() {
        for (version, format) in [
            ("AC1015", FileFormat::Dwg2000),
            ("AC1018", FileFormat::Dwg2004),
            ("AC1021", FileFormat::Dwg2007),
        ] {
            let bytes = fixture(&format!("dwg/valid/ezdxf-r12-three-lines.{version}.dwg"));
            let mut cad = CadSession::new();
            // The name is the host's: a misleading extension changes nothing.
            cad.load_drawing(&bytes, "plan.dxf");
            let drawing = cad.session.drawing();
            assert_eq!(drawing.len(), 3, "{version}");
            assert!(
                drawing
                    .layers()
                    .iter()
                    .any(|(_, layer)| layer.name == "Defpoints"),
                "{version}"
            );
            let state = cad.session.ui_state();
            assert_eq!(state.document_name.as_deref(), Some("plan.dxf"));
            assert_eq!(state.document_format, format, "{version}");
            assert_eq!(
                state.import_report.map(|r| r.lines[0].clone()),
                Some("Imported 3 lines.".into())
            );
        }
    }

    #[test]
    fn a_dxf_still_opens_and_a_later_dwg_is_refused_by_name() {
        let mut cad = CadSession::new();
        cad.load_drawing(&fixture("dxf/valid/ezdxf-r12-three-lines.dxf"), "plan.dwg");
        assert_eq!(cad.session.drawing().len(), 3);
        assert_eq!(cad.session.ui_state().document_format, FileFormat::Dxf);

        cad.load_drawing(b"AC1024 is a later release", "new.dwg");
        assert_eq!(cad.session.drawing().len(), 3, "the drawing is kept");
        let log = cad.session.log_since(0);
        assert!(
            log.last().is_some_and(|line| {
                line.starts_with("Cannot open new.dwg: DWG version AC1024 is not supported")
            }),
            "{log:?}"
        );
    }

    /// #319: SAVEAS with each format keyword asks the host for that format
    /// and the bytes carry its signature; SAVE after opening a DWG writes
    /// its version; a new drawing saves as DXF.
    #[test]
    fn saveas_writes_the_format_asked_for_and_save_keeps_the_files_own() {
        let signature = |bytes: &[u8]| String::from_utf8_lossy(&bytes[..6]).into_owned();
        let mut cad = CadSession::new();
        cad.submit_text("L");
        cad.submit_text("0,0");
        cad.submit_text("10,10");
        cad.submit_text("");
        for (keyword, id, head) in [
            (
                "DXF", "dxf", "  0
SE",
            ),
            ("DWG2000", "dwg2000", "AC1015"),
            ("DWG2004", "dwg2004", "AC1018"),
            ("DWG2007", "dwg2007", "AC1021"),
        ] {
            cad.submit_text("SAVEAS");
            cad.submit_text(keyword);
            let request = cad.take_host_request();
            assert_eq!(request & 0xff, 2, "{keyword}");
            let format = FileFormat::ALL[(request >> 8) as usize - 1];
            assert_eq!(format.id(), id);
            assert_eq!(signature(&cad.snapshot_file(id)), head, "{keyword}");
            assert_eq!(cad.export_losses(id), Vec::<String>::new());
        }
        // A new drawing saves as DXF.
        cad.submit_text("SAVE");
        assert_eq!(cad.take_host_request(), 3);
        assert_eq!(cad.session.document_format(), FileFormat::Dxf);
        // One opened from a DWG saves as its version, and a name with a
        // space in it is the drawing's name from then on.
        for (version, id) in [
            ("AC1015", "dwg2000"),
            ("AC1018", "dwg2004"),
            ("AC1021", "dwg2007"),
        ] {
            let bytes = fixture(&format!("dwg/valid/ezdxf-r12-three-lines.{version}.dwg"));
            cad.load_drawing(&bytes, "plan.dwg");
            cad.submit_text("SAVE");
            assert_eq!(cad.take_host_request(), 3);
            let format = cad.session.document_format();
            assert_eq!(format.id(), id);
            assert_eq!(signature(&cad.snapshot_file(format.id())), version);
            cad.mark_exported(Some("my plan.dwg".into()), id);
            let state = cad.session.ui_state();
            assert_eq!(state.document_name.as_deref(), Some("my plan.dwg"));
            assert!(!state.unsaved_changes);
        }
        assert!(cad.snapshot_file("gif").is_empty());
    }

    /// A DWG with content the drawing cannot hold reports it like a DXF
    /// does: the report, and the losses an export will not include.
    #[test]
    fn a_dwg_with_unsupported_content_is_lossy_like_a_dxf() {
        let mut cad = CadSession::new();
        cad.load_drawing(
            &fixture("dwg/valid/crafted-mline-entity.AC1015.dwg"),
            "mline.dwg",
        );
        let state = cad.session.ui_state();
        assert!(
            state
                .import_report
                .is_some_and(|r| r.lines.iter().any(|l| l.contains("MLINE")))
        );
        assert!(
            state.export_losses.iter().any(|l| l.contains("MLINE")),
            "{:?}",
            state.export_losses
        );
    }
    /// #254: a proposal through the calls the host makes: the drawing is
    /// untouched until Accept, which is one undo; an invalid document is
    /// refused in the log.
    #[test]
    fn a_proposal_is_proposed_accepted_and_undone_as_one_step() {
        let mut cad = CadSession::new();
        let revision = cad.session.drawing().revision();
        cad.propose(
            r#"{"version": 1, "operations": [
                {"op": "circle", "center": [0, 0], "radius": 5},
                {"op": "line", "points": [[0, 0], [10, 0]]}
            ]}"#,
        );
        assert_eq!(cad.session.drawing().revision(), revision);
        assert_eq!(
            cad.session.proposal().map(|p| p.summary().headline),
            Some("Adds 2 objects".to_owned())
        );
        cad.accept_proposal();
        assert_eq!(cad.session.drawing().len(), 2);
        cad.submit_text("U");
        assert!(cad.session.drawing().is_empty());

        cad.propose(r#"{"version": 1, "operations": [{"op": "circle"}]}"#);
        assert!(cad.session.proposal().is_none());
        let log = cad.session.log_since(0);
        assert!(
            log.last()
                .is_some_and(|l| l.starts_with("The proposal is not valid:")),
            "{log:?}"
        );
    }
}
