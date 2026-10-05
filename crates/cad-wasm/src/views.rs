//! Zero-copy typed-array views over Rust-owned buffers (ADR-0002 §4).

use js_sys::{Float32Array, Uint8Array, Uint16Array, Uint32Array};

/// A `Float32Array` aliasing `data` inside WASM linear memory.
///
/// The view is valid only until the next call into the module: any allocation
/// may grow memory and detach it. The TypeScript wrapper therefore consumes
/// views immediately and re-creates them on every pull.
#[allow(unsafe_code)]
pub(crate) fn f32_view(data: &[f32]) -> Float32Array {
    // SAFETY: the returned view is handed straight to JavaScript, which copies
    // it into a GPU buffer before calling back into WASM. No Rust code runs —
    // and so no allocation can happen — while the view is alive.
    unsafe { Float32Array::view(data) }
}

/// A `Uint32Array` aliasing `data`. Same lifetime rule as [`f32_view`].
#[allow(unsafe_code)]
pub(crate) fn u32_view(data: &[u32]) -> Uint32Array {
    // SAFETY: see `f32_view`.
    unsafe { Uint32Array::view(data) }
}

/// A `Uint16Array` aliasing `data`. Same lifetime rule as [`f32_view`].
#[allow(unsafe_code)]
pub(crate) fn u16_view(data: &[u16]) -> Uint16Array {
    // SAFETY: see `f32_view`.
    unsafe { Uint16Array::view(data) }
}

/// A `Uint8Array` aliasing `data`. Same lifetime rule as [`f32_view`].
#[allow(unsafe_code)]
pub(crate) fn u8_view(data: &[u8]) -> Uint8Array {
    // SAFETY: see `f32_view`.
    unsafe { Uint8Array::view(data) }
}
