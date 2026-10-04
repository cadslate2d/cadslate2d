//! The JS/WASM boundary. Marshalling only; no CAD logic (ADR-0001, ADR-0002).

use wasm_bindgen::prelude::*;

/// Runs once when the module is instantiated.
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// Version of the CAD core, shown by the shell to prove the pipeline is connected.
#[wasm_bindgen]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
