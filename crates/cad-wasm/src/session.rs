//! The one exported handle (ADR-0002 §1).

use cad_commands::{
    Dirty, HostRequest, KeyAction, LayerRow, Modifiers, PointerButton, SelectionSummary, Session,
    Toggle, UiState,
};
use cad_core::{ClipTransform, ScreenPoint};
use cad_render::{OverlayFrame, SceneCache};
use js_sys::{Float32Array, Uint8Array, Uint16Array, Uint32Array};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::views::{f32_view, u8_view, u16_view, u32_view};

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
    toggles: u32,
    document_name: Option<String>,
    unsaved_changes: bool,
    revision: f64,
    export_losses: Vec<String>,
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
            toggles: state.toggles,
            document_name: state.document_name,
            unsaved_changes: state.unsaved_changes,
            revision: state.revision as f64,
            export_losses: state.export_losses,
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
            self.overlay
                .rebuild(&items, self.session.drawing(), self.session.view());
        }
    }
}

#[wasm_bindgen]
impl CadSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            session: Session::new(),
            scene: SceneCache::new(),
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
    /// its file. Taking it clears it.
    pub fn take_host_request(&mut self) -> u32 {
        match self.session.take_host_request() {
            None => 0,
            Some(HostRequest::OpenFile) => 1,
            Some(HostRequest::ExportFile) => 2,
            Some(HostRequest::SaveFile) => 3,
        }
    }

    /// Replaces the drawing with the contents of a DXF file. On failure the
    /// current drawing is kept and the reason is written to the command log.
    pub fn load_dxf(&mut self, bytes: &[u8], name: &str) -> u32 {
        let dirty = match cad_dxf::import(bytes) {
            Ok(import) => self.session.open_drawing(
                import.drawing,
                name,
                import.report.summary(),
                import.report.losses(),
            ),
            Err(error) => self.session.notify(format!("Cannot open {name}: {error}.")),
        };
        self.report(dirty)
    }

    /// Takes the import report off the screen (#157).
    pub fn dismiss_import_report(&mut self) -> u32 {
        let dirty = self.session.dismiss_import_report();
        self.report(dirty)
    }

    /// The drawing as ASCII DXF, for the host to keep as an autosave (#158):
    /// marks nothing, so what the user has and has not exported stays true.
    pub fn snapshot_dxf(&self) -> String {
        cad_dxf::export(self.session.drawing())
    }

    /// Records that the host has saved the drawing (the text of
    /// `snapshot_dxf`) — to `name`, when it has a file now (#159).
    pub fn mark_exported(&mut self, name: Option<String>) -> u32 {
        let dirty = self.session.mark_exported(name.as_deref());
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
        // Revisions are small sequential integers; f64 represents them exactly.
        self.session.drawing().shown_revision().as_u64() as f64
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
        u8_view(self.scene.scene(self.session.drawing()).styles())
    }

    /// The pattern table (ADR-0015): ten floats per row — the period, the
    /// number of lengths, then dash, gap, dash, gap … in drawing units with
    /// `LTSCALE` applied, zeros after the last. Row 0 is a continuous line.
    /// Changes with the style table. A view into WASM memory — consume
    /// before the next call.
    pub fn scene_patterns(&mut self) -> Float32Array {
        f32_view(self.scene.scene(self.session.drawing()).patterns())
    }

    /// Per committed segment instance, the distance along its entity's
    /// path at its start, in drawing units: where its linetype pattern is
    /// (ADR-0015). Changes with the geometry. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_segment_distances(&mut self) -> Float32Array {
        f32_view(
            self.scene
                .scene(self.session.drawing())
                .frame()
                .segment_distances(),
        )
    }

    /// Per committed arc instance, two floats: the distance along its
    /// entity's path at the arc's counter-clockwise start, and `1` or `-1`
    /// for the direction the path runs round it. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_arc_distances(&mut self) -> Float32Array {
        f32_view(
            self.scene
                .scene(self.session.drawing())
                .frame()
                .arc_distances(),
        )
    }

    /// One style index per committed segment instance. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_segment_styles(&mut self) -> Uint16Array {
        u16_view(self.scene.scene(self.session.drawing()).segment_styles())
    }

    /// One style index per committed arc instance. A view into WASM memory —
    /// consume before the next call.
    pub fn scene_arc_styles(&mut self) -> Uint16Array {
        u16_view(self.scene.scene(self.session.drawing()).arc_styles())
    }

    /// Committed segment instances: `x0, y0, x1, y1` per segment, relative to
    /// the scene origin. A view into WASM memory — consume before the next call.
    pub fn scene_segments(&mut self) -> Float32Array {
        f32_view(self.scene.scene(self.session.drawing()).frame().segments())
    }

    /// Committed batches: `style, first, count` per batch. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_batches(&mut self) -> Uint32Array {
        u32_view(
            self.scene
                .scene(self.session.drawing())
                .frame()
                .batch_words(),
        )
    }

    /// Committed arc instances: `cx, cy, radius, start, sweep` per arc, relative
    /// to the scene origin. A view into WASM memory — consume before the next call.
    pub fn scene_arcs(&mut self) -> Float32Array {
        f32_view(self.scene.scene(self.session.drawing()).frame().arcs())
    }

    /// Committed arc batches: `style, first, count` per batch. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_arc_batches(&mut self) -> Uint32Array {
        u32_view(
            self.scene
                .scene(self.session.drawing())
                .frame()
                .arc_batch_words(),
        )
    }

    /// Clip transform for the committed scene: `[scale_x, scale_y, translate_x,
    /// translate_y]`, a copy. Changes with the view; the scene does not.
    pub fn scene_transform(&mut self) -> Vec<f32> {
        let origin = self.scene.scene(self.session.drawing()).frame().origin();
        transform_words(self.session.view().clip_transform(origin))
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
        f32_view(self.scene.scene(self.session.drawing()).frame().triangles())
    }

    /// Committed triangle batches: `style, first, count` per batch. A view
    /// into WASM memory — consume before the next call.
    pub fn scene_triangle_batches(&mut self) -> Uint32Array {
        u32_view(
            self.scene
                .scene(self.session.drawing())
                .frame()
                .triangle_batch_words(),
        )
    }

    /// One style index per committed triangle instance. A view into WASM
    /// memory — consume before the next call.
    pub fn scene_triangle_styles(&mut self) -> Uint16Array {
        u16_view(self.scene.scene(self.session.drawing()).triangle_styles())
    }

    /// Which committed instances changed since this was last called
    /// (ADR-0022): `kind, first, count` per range, kind 0 segments, 1 arcs,
    /// 2 triangles, in instance units. After a whole rebuild, one range
    /// per kind over everything. Read it before the scene buffers of the
    /// same frame and upload those ranges with `bufferSubData`; or the
    /// whole buffers, when the GPU holds no scene yet. Owned: safe to keep.
    pub fn scene_dirty_ranges(&mut self) -> Uint32Array {
        self.scene.scene(self.session.drawing());
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
