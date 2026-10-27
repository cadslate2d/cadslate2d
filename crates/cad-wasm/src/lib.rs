//! The JS/WASM boundary. Marshalling only; no CAD logic (ADR-0001, ADR-0002).
//!
//! The call pattern is deliberately coarse: one semantic call per browser
//! event, each returning a dirty-flag bitmask; the host then pulls only what
//! changed, in bulk. There are no per-entity or per-vertex accessors and Rust
//! never calls back into JavaScript to draw.

mod session;
mod views;

use std::cell::RefCell;

use wasm_bindgen::prelude::*;

pub use session::CadSession;

thread_local! {
    /// What stopped the engine: the first panic's message (#176).
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Runs once when the module is instantiated.
#[wasm_bindgen(start)]
fn start() {
    // A panic aborts the call it happens in and leaves the session
    // unusable. The hook keeps the message for the host to read on its
    // next call — the one call it then makes — and prints it as before.
    std::panic::set_hook(Box::new(|info| {
        PANIC.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                *slot = Some(info.to_string());
            }
        });
        console_error_panic_hook::hook(info);
    }));
}

/// The message of the panic that stopped the engine, if one has (#176).
/// Reads one static and nothing of the session, so it is safe to call
/// after the session is not.
#[wasm_bindgen]
pub fn panic_message() -> Option<String> {
    PANIC.with(|slot| slot.borrow().clone())
}

/// Panics, so the recovery path can be exercised end to end (#176). The
/// shell reaches it only through the debug handle.
#[wasm_bindgen]
pub fn panic_for_test() {
    panic!("asked to fail, for the test");
}

/// Version of the CAD core.
/// One pattern of the program's hatch table as the hatch dialog shows it:
/// its name, what it is for, and its swatch's lines, `[x0, y0, x1, y1, …]`
/// in a unit square, y up.
#[derive(serde::Serialize)]
struct HatchSwatchDto {
    name: String,
    description: String,
    solid: bool,
    lines: Vec<f32>,
}

/// Every hatch pattern with its swatch, in one call when the dialog opens
/// (#346): static program data, nothing of a drawing.
#[wasm_bindgen]
pub fn hatch_swatches() -> Result<JsValue, JsError> {
    let swatches: Vec<HatchSwatchDto> = cad_document::hatch_patterns()
        .iter()
        .map(|pattern| HatchSwatchDto {
            name: pattern.name.clone(),
            description: pattern.description.clone(),
            solid: pattern.is_solid(),
            lines: cad_document::swatch(&pattern.name)
                .unwrap_or_default()
                .iter()
                .flat_map(|line| [line.start, line.end])
                .flat_map(|q| [q.x, q.y])
                .map(|v| v as f32)
                .collect(),
        })
        .collect();
    Ok(serde_wasm_bindgen::to_value(&swatches)?)
}

#[wasm_bindgen]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
