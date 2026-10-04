//! The one exported handle (ADR-0002 §1).

use cad_commands::{
    Dirty, HostRequest, KeyAction, Modifiers, PointerButton, Session, Toggle, UiState,
};
use cad_core::{ClipTransform, ScreenPoint};
use cad_render::{OverlayFrame, SceneCache};
use js_sys::{Float32Array, Uint32Array};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::views::{f32_view, u32_view};

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
    selection_count: u32,
    space_accepts: bool,
    can_undo: bool,
    can_redo: bool,
    toggles: u32,
    document_name: Option<String>,
    unsaved_changes: bool,
    export_is_lossy: bool,
}

impl From<UiState> for UiStateDto {
    fn from(state: UiState) -> Self {
        Self {
            prompt: state.prompt,
            command: state.command,
            log_total: state.log_total as f64,
            cursor: state.cursor,
            selection_count: state.selection_count as u32,
            space_accepts: state.space_accepts,
            can_undo: state.can_undo,
            can_redo: state.can_redo,
            toggles: state.toggles,
            document_name: state.document_name,
            unsaved_changes: state.unsaved_changes,
            export_is_lossy: state.export_is_lossy,
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
    /// `2` export the drawing. Taking it clears it.
    pub fn take_host_request(&mut self) -> u32 {
        match self.session.take_host_request() {
            None => 0,
            Some(HostRequest::OpenFile) => 1,
            Some(HostRequest::ExportFile) => 2,
        }
    }

    /// Replaces the drawing with the contents of a DXF file. On failure the
    /// current drawing is kept and the reason is written to the command log.
    pub fn load_dxf(&mut self, bytes: &[u8], name: &str) -> u32 {
        let dirty = match cad_dxf::import(bytes) {
            Ok(import) => {
                let lossy = import.report.has_unsupported_content();
                self.session
                    .open_drawing(import.drawing, name, lossy, import.report.summary())
            }
            Err(error) => self.session.notify(format!("Cannot open {name}: {error}.")),
        };
        self.report(dirty)
    }

    /// The drawing as ASCII DXF. Marks the drawing as saved.
    pub fn export_dxf(&mut self) -> String {
        let text = cad_dxf::export(self.session.drawing());
        let dirty = self.session.mark_exported();
        self.report(dirty);
        text
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

    /// Identifies the committed scene. Re-upload committed GPU buffers only
    /// when this differs from the revision last uploaded. Cheap: does not
    /// build the scene.
    pub fn scene_revision(&self) -> f64 {
        // Revisions are small sequential integers; f64 represents them exactly.
        self.session.drawing().revision().as_u64() as f64
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
