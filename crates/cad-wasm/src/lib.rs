//! The JS/WASM boundary. Marshalling only; no CAD logic (ADR-0001, ADR-0002).
//!
//! The call pattern is deliberately coarse: one semantic call per browser
//! event, each returning a dirty-flag bitmask; the host then pulls only what
//! changed, in bulk. There are no per-entity or per-vertex accessors and Rust
//! never calls back into JavaScript to draw.

mod session;
mod views;

use wasm_bindgen::prelude::*;

pub use session::CadSession;

/// Runs once when the module is instantiated.
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// Version of the CAD core.
#[wasm_bindgen]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
