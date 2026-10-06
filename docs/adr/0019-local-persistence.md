# ADR-0019: Local persistence — the autosave lives in IndexedDB

**Status:** Accepted (2026-10-06)  
**Date:** 2026-10-06  
**Kind:** Evidence (resolves DESIGN §25.1, PROVISIONAL)  
**Design refs:** §25.1, §25.2, §25.3, §33, §43, §45

## Context

CadSlate2D is local-first and has no account (§25, §43): the only copy of unexported work is the tab it is drawn in. A tab closed by mistake, a browser that crashes, a laptop that dies — and the drawing since the last export is gone. §25 asks for browser-local persistence; §25.1 leaves the mechanism PROVISIONAL between IndexedDB, the OPFS async API, and a hybrid, and §25.2 forbids introducing a Worker just to get synchronous OPFS handles.

What is kept is the drawing as our own DXF: after ADR-0018 the file holds everything the drawing does, so no second format is needed, and recovering is an ordinary open.

## The candidates

**IndexedDB.** A transaction, one `put` of the whole text under one key. Available on every browser's main thread since 2012. The text is structured-cloned into the store.

**OPFS, async API.** `navigator.storage.getDirectory()`, a file handle, `createWritable()`, `write()`, `close()`. A real file, streamable. The writable stream is not available on the main thread in Safari, which has OPFS only through synchronous access handles in a Worker — the coupling §25.2 warns about.

**Hybrid.** OPFS where it has the writable stream, IndexedDB elsewhere: two code paths and two sets of failure modes for one slot of storage.

## Evidence

`web/src/app/persistence-probe.ts` (`pnpm --dir web bench:persistence`, after a build) writes a drawing's DXF to each store five times on the main thread and reads it back, reporting the median write and read times and the longest gap between two animation frames during a write — how long the main thread was held. The drawings are the generated benchmark drawings of `benches/` at 10,000, 50,000, and 100,000 lines: 1.6 MB, 8.2 MB, and 16.4 MB of DXF. Headless Chromium and Firefox on this machine; WebKit is not installed here.

| Drawing | Store | Chromium write | Chromium read | Chromium longest gap | Firefox write | Firefox read | Firefox longest gap |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 10k lines, 1.6 MB | IndexedDB | 4.8 ms | 2.8 ms | 3.2 ms | 18 ms | 6 ms | 14 ms |
| | OPFS | 5.5 ms | 3.4 ms | 4.8 ms | 35 ms | 4 ms | 17 ms |
| 50k lines, 8.2 MB | IndexedDB | 21 ms | 11 ms | 23 ms | 64 ms | 27 ms | 18 ms |
| | OPFS | 19 ms | 13 ms | 15 ms | 41 ms | 15 ms | 17 ms |
| 100k lines, 16.4 MB | IndexedDB | 38 ms | 18 ms | 17 ms | 113 ms | 57 ms | 18 ms |
| | OPFS | 33 ms | 27 ms | 17 ms | 36 ms | 37 ms | 18 ms |

Both stores write the largest drawing in well under a frame's worth of main-thread time: the longest gap between frames during any write was 23 ms, about one frame at 60 Hz, for both. The wall time from the call to the write being durable is tens of milliseconds in Chromium for both, and in Firefox IndexedDB is the slower by up to 80 ms at 16 MB — once per autosave, after a pause in drawing, and not on the main thread's clock.

What the measurement does not cover: producing the DXF. The export runs in WebAssembly on the main thread and is not asynchronous; for 100,000 lines it is the dominant cost of an autosave, and a Worker (§37) is the only way to take it off the thread. It is the same export the user triggers with `DXFOUT`, and it happens once per pause in drawing.

## Decision

### 1. The autosave is one IndexedDB record

One database, one object store, one key, one record: the drawing's DXF, the file name it came from or went to, the time, and whether a file holds this state too. Written whole with one `put`, read whole with one `get`. IndexedDB because it is on every main thread, including Safari's; because one record under one key is all the structure there is; and because the measured cost difference is tens of milliseconds once per pause, which is not worth a second code path. No Worker is introduced (§25.2). The hybrid is rejected for the same reason the OPFS path alone is: it is a second way to do one thing.

### 2. The host decides when, Rust supplies what

Rust knows nothing of storage. `UiState` carries the drawing's revision — it changes on every commit, undo, and redo — and the host autosaves when it changes, one second after the last change, through `snapshot_dxf`, which exports without marking the drawing exported. A pointer move, a pan, a zoom change no revision and cost nothing. Exporting (`DXFOUT`) and opening a file write the record as *exported*: a file holds the drawing, so a reload has nothing to recover.

The issue (#158) sketched a `HostRequest` for this, as for files. A `HostRequest` is a command asking the host to act; the autosave is the host's own policy about the host's own storage, so it is the host that watches the revision. Nothing is asked twice and nothing contends for the one request slot.

### 3. Recovery is offered once, and is an open

On start the host looks at the record. One that a file does not hold — a tab closed before exporting — is offered in a narrow dialog (§5): the name and the time, *Recover* or *Discard*. Recovering is `loadDxf` of the kept text, under the kept name or `recovered.dxf`; the record stays unexported, so a second crash offers it again. Discarding clears the slot. A record a file holds is not offered.

### 4. Nothing leaves the browser

IndexedDB is the page's own storage; the end-to-end test records every request the page makes while drawing, autosaving, reloading, and recovering, and finds none but the download's `blob:` URL (§43, §45).

## Alternatives considered

- **OPFS alone.** As fast or faster, and a real file. Rejected for Safari's main thread and for the Worker it would eventually pull in.
- **`localStorage`.** Synchronous and simple, but capped at a few megabytes in every browser: a 50,000-line drawing does not fit.
- **Autosave by time rather than by change.** A timer every N seconds writes the drawing whether or not it changed. Rejected: the revision is already there, and writing an unchanged 16 MB drawing every half minute is work for nothing.
- **Several slots, or a history of autosaves.** One slot answers the question §25 asks — the work since the last export. More is a native project format's job (§39).

## Consequences

- `HostServices` gains `autosave: AutosaveStore` (`load`, `save`, `clear`); the browser host uses `indexedDbAutosave`, tests a fake in memory.
- `CadApp` schedules the autosave from the frame that reports a changed scene, through an injectable timer, so the tests fire time rather than wait for it; `start()` offers a recoverable record; `recover()` and `discardRecovery()` answer it.
- `UiState.revision` and `CadSession::snapshot_dxf` are the two additions to the WASM boundary.
- The `beforeunload` guard stays: the autosave is a net, not a licence to close without exporting.

## Revisit when

- A drawing of 100,000 entities autosaves and the export's main-thread time is felt: that is the Worker's case (§37), and the store does not change.
- Safari gains the OPFS writable stream on the main thread, or a reason appears to keep more than one slot: the interface is one object with three methods, and a second implementation slots behind it.
