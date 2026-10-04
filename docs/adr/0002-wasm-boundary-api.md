# ADR-0002: WASM boundary — events in, dirty flags out, pulled frames

**Status:** Proposed  
**Date:** 2026-10-04  
**Kind:** Bootstrap (fills a gap in the baseline; changes no LOCKED decision)  
**Design refs:** §6, §7, §12.1–12.4, §12.6, §13, §28, §41

## Context

The baseline locks that the browser/WASM boundary is coarse (§6), that render data crosses in batches (§12.4), and that React holds no drawing representation (§7.2). It does not define the shape of the API. Two questions are left open and both block M0:

1. How does browser input reach the command state machine?
2. How do prompt text, status, and render data get back out without fine-grained traffic?

The baseline also assigns "command-line presentation" to TypeScript and "command state" to Rust without saying which side holds the text the user is typing.

The exact packed-buffer representation and transport remain PROVISIONAL (§12.2, §12.6). This ADR fixes the call pattern, not the byte layout.

## Decision

### 1. One handle

`cad-wasm` exports one class, `CadSession`, wrapping one `cad_commands::Session` plus its render caches. JS never receives references to `Drawing`, entities, or any other core object.

### 2. Input: one semantic call per browser event

```text
pointer_move(x, y, modifiers)
pointer_down(x, y, button, modifiers)
pointer_up(x, y, button, modifiers)
wheel(x, y, delta, modifiers)
key(action, modifiers)        non-text keys: Enter, Escape, F-keys, Delete, undo/redo chords
submit_text(line)             a completed command-line entry
resize(width, height, dpr)
load_dxf(bytes) → ImportReport
export_dxf() → bytes
```

Coordinates are CSS pixels relative to the canvas. TypeScript coalesces pointer moves to at most one call per animation frame.

### 3. Output: dirty flags, then pull

Every input call returns a `u32` bitmask:

```text
SCENE    committed scene revision changed
OVERLAY  transient/overlay geometry changed
VIEW     pan/zoom/size changed
UI       prompt, log, status, or toggles changed
```

TypeScript schedules one animation frame when any flag is set and pulls only what is flagged:

```text
scene()     committed geometry + batch table + revision
overlay()   transient geometry + batch table
view()      view uniforms (see ADR-0003)
ui_state()  prompt, options, new log lines, toggles, cursor world position, selection count
```

Rust never calls into JS to render or to update UI. The only JS imports are diagnostics (panic hook, logging).

### 4. Geometry crosses as views into WASM memory

`scene()` and `overlay()` return typed-array views over buffers that Rust owns and reuses across frames. A view is valid only until the next call into the session. The adapter uploads to the GPU immediately and never stores a view. Views are re-created on every pull, because WASM memory growth detaches earlier ones.

This is the M0 starting point. The transport measurement (§12.6) may replace it through its own ADR.

### 5. UI state crosses as one small serialized object

`ui_state()` returns a plain object (serde). React subscribes to it through a single external store. It contains presentation data only; nothing in it can reconstruct drawing geometry.

### 6. The in-progress command-line text lives in TypeScript

The edit buffer is a real `<input>` so caret, clipboard, and IME behave natively. TypeScript does not interpret it. On Enter — or Space when `ui_state.space_accepts` is true — the raw string goes to `submit_text`. Rust tokenizes, resolves against the active command's expected input, and owns the prompt, diagnostics, and history. History recall is a call into Rust that returns the recalled string.

Keystrokes typed while the canvas has focus are routed to the command-line input, so typing never requires clicking the command line first.

### 7. Forbidden patterns

- per-entity or per-vertex getters (`entity(i)`, `vertex(i)`);
- GL calls issued from Rust (§12.1);
- JS callbacks invoked from inside an input call;
- any React state that mirrors entities, selection membership, or grips.

## Alternatives considered

- **Push model: Rust invokes JS callbacks when state changes.** Rejected: re-entrancy hazards and it invites fine-grained calls.
- **Return the whole frame from every input call.** Rejected: pointer moves would marshal scene data that has not changed, defeating §12.3.
- **JSON for geometry.** Rejected on allocation and parse cost.
- **Rust owns the text buffer; forward every keystroke.** Rejected for now: requires reimplementing caret, selection, clipboard, and IME. Revisit if dynamic input (§13.5) needs per-keystroke interpretation.
- **Multiple exported objects (document, view, commands).** Rejected: widens the boundary and lets JS sequence core operations.

## Consequences

- The API surface is small enough to review for coarseness in one sitting (§28).
- The view-lifetime rule is a sharp edge. The adapter must be the only code that touches views, and a test must cover memory growth between pulls.
- Space-as-Enter depends on a Rust-provided flag, so text-entry commands (M4) need no boundary change.
- The headless Rust harness and the browser drive the same `Session` methods, so E2E tests mostly verify wiring.

## Revisit when

- The transport measurement shows view creation or copying dominates frame time.
- A worker architecture moves the session off the main thread (§37).
- Dynamic input (§13.5) is scheduled.
