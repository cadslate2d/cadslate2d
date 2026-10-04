//! The one exported handle (ADR-0002 §1).

use cad_commands::{Modifiers, PointerButton, Session};
use cad_core::ScreenPoint;
use cad_render::SceneCache;
use js_sys::{Float32Array, Uint32Array};
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

/// DOM `MouseEvent.button` numbering. Unknown buttons are ignored by callers.
fn button(code: u32) -> Option<PointerButton> {
    match code {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Middle),
        2 => Some(PointerButton::Secondary),
        _ => None,
    }
}

/// One open drawing. JavaScript never sees the document or its entities —
/// only this handle, the dirty flags it returns, and bulk frame data.
#[wasm_bindgen]
#[derive(Debug)]
pub struct CadSession {
    session: Session,
    scene: SceneCache,
    scene_batch_words: Vec<u32>,
}

impl Default for CadSession {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl CadSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            session: initial_session(),
            scene: SceneCache::new(),
            scene_batch_words: Vec::new(),
        }
    }

    // ── input: one call per browser event, each returning dirty flags ──

    /// Canvas size in CSS pixels.
    pub fn resize(&mut self, width: f64, height: f64) -> u32 {
        self.session.resize(width, height).bits()
    }

    pub fn pointer_move(&mut self, x: f64, y: f64, modifier_bits: u32) -> u32 {
        self.session
            .pointer_move(ScreenPoint::new(x, y), modifiers(modifier_bits))
            .bits()
    }

    pub fn pointer_down(&mut self, x: f64, y: f64, button_code: u32, modifier_bits: u32) -> u32 {
        button(button_code).map_or(0, |button| {
            self.session
                .pointer_down(ScreenPoint::new(x, y), button, modifiers(modifier_bits))
                .bits()
        })
    }

    pub fn pointer_up(&mut self, x: f64, y: f64, button_code: u32, modifier_bits: u32) -> u32 {
        button(button_code).map_or(0, |button| {
            self.session
                .pointer_up(ScreenPoint::new(x, y), button, modifiers(modifier_bits))
                .bits()
        })
    }

    /// `delta_y` in CSS pixels, as in `WheelEvent.deltaY` with pixel delta mode.
    pub fn wheel(&mut self, x: f64, y: f64, delta_y: f64, modifier_bits: u32) -> u32 {
        self.session
            .wheel(ScreenPoint::new(x, y), delta_y, modifiers(modifier_bits))
            .bits()
    }

    // ── output: pulled in bulk, only when flagged ──

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
        let frame = self.scene.scene(self.session.drawing()).frame();
        frame.write_batch_words(&mut self.scene_batch_words);
        u32_view(&self.scene_batch_words)
    }

    /// Clip transform for the committed scene: `[scale_x, scale_y, translate_x,
    /// translate_y]`, a copy. Changes with the view; the scene does not.
    pub fn scene_transform(&mut self) -> Vec<f32> {
        let origin = self.scene.scene(self.session.drawing()).frame().origin();
        let transform = self.session.view().clip_transform(origin);
        vec![
            transform.scale[0],
            transform.scale[1],
            transform.translate[0],
            transform.translate[1],
        ]
    }
}

#[cfg(not(feature = "demo-scene"))]
fn initial_session() -> Session {
    Session::new()
}

/// A recognisable test pattern, built through the public import path.
#[cfg(feature = "demo-scene")]
fn initial_session() -> Session {
    use cad_core::Point2;
    use cad_document::{Drawing, Entity, LineEntity};

    let line = |x0: f64, y0: f64, x1: f64, y1: f64| {
        Entity::Line(LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    };
    let mut lines = vec![
        line(-150.0, -100.0, 150.0, -100.0),
        line(150.0, -100.0, 150.0, 100.0),
        line(150.0, 100.0, -150.0, 100.0),
        line(-150.0, 100.0, -150.0, -100.0),
        line(-150.0, -100.0, 150.0, 100.0),
        line(-150.0, 100.0, 150.0, -100.0),
    ];
    // A fan of lines at many angles, to check stroke width is uniform.
    for step in 0..24 {
        let angle = f64::from(step) * std::f64::consts::TAU / 24.0;
        lines.push(line(0.0, 0.0, 80.0 * angle.cos(), 80.0 * angle.sin()));
    }
    Session::with_drawing(Drawing::from_entities(lines))
}
