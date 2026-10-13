# ADR-0024: No worker; the autosave is written in slices on the main thread

**Status:** Accepted (2026-10-12)  
**Date:** 2026-10-12  
**Kind:** Evidence (resolves §35 "worker experiments" and §37's "a worker must be justified by profiling"; #178)  
**Design refs:** §8, §25, §35, §37, §42; ADR-0002, ADR-0019

## Context

Three things hold the main thread for as long as the drawing is large: opening a DXF (parse and build), exporting one (DXFOUT), and the autosave, which is an export a second after every pause in editing (ADR-0019). §37 says a worker must be justified by profiling, not assumed. #178 asks for the measurements and the smallest move that fixes what they show.

## The measurements

`pnpm --dir web bench:main-thread` (`probeMainThread`, on the debug handle), the mixed drawings of #171 opened, exported, and autosaved in a throwaway engine, each measured as the time the main thread is held — for the autosave, the longest gap between two animation frames while it runs. Headless Chromium and Firefox on the development machine (Intel i7-12650H, WSL2), 2026-10-12, with two cores busy fuzzing.

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| DXF written | 3.2 MB | 16.2 MB | 32.5 MB |
| Open, Firefox / Chromium | 64 / 98 ms | 313 / 305 ms | 447 / 436 ms |
| Export, Firefox / Chromium | 55 / 46 ms | 269 / 256 ms | 531 / 416 ms |
| Autosave, whole: the thread held, Firefox / Chromium | 50 / 70 ms | 268 / 244 ms | 539 / 424 ms |
| Autosave, whole: until durable, Firefox / Chromium | 155 / 90 ms | 620 / 318 ms | 1335 / 536 ms |

Natively the export is 263 ms for the 32.5 MB at 100k, 120 MB/s: the writer is not slow, the file is large. The IndexedDB put holds the thread for a frame at most (ADR-0019); the rest is the export.

**After:** the autosave written in slices of 2,000 records, a turn of the event loop apart, the same drawings (one core busy fuzzing):

| | 10k | 50k | 100k |
| --- | --- | --- | --- |
| Autosave, sliced: the thread held, Firefox / Chromium | 18 / 18 ms | 42 / 48 ms | 64 / 54 ms |
| Autosave, sliced: until durable, Firefox / Chromium | 73 / 51 ms | 463 / 389 ms | 935 / 704 ms |

The thread is never held longer than 64 ms, which is the last step — the file taken as one 32 MB buffer and put into IndexedDB, whose structured clone is a copy — against half a second before; the slices themselves are ten to fifteen milliseconds. The autosave takes longer to become durable, as it shares the thread with everything else, which is the point.

## Decision

### 1. The threshold

A main-thread task is felt at 100 ms (RAIL's response budget) and breaks animation at 16 ms. A task the user asked for and sees the result of — an open, a DXFOUT — may take up to a second at the top of §42's range, 100k entities, as it does. A task that recurs on its own while the user works may not: the autosave fires after every pause, and a quarter-second freeze at 50k (§42: "smooth for normal editing") or half a second at 100k, landing just as the user resumes, is exactly the kind of jank §12 was written to forbid.

### 2. No worker

A worker cannot export what it does not have, and the drawing is owned by the main thread's engine (§8). Giving a worker the drawing means either serialising it — which is the cost being moved — or keeping a second engine in step with every transaction, a replica with its own WASM instance and a change stream between the two. That is a day or more of new infrastructure for the one recurring cost, and it moves the export, not the open: the open is the parse, which must end up in the main thread's engine whatever thread parses it, short of transferring a built drawing, which no format exists for.

### 3. The autosave is written in slices

`cad_dxf::Export` is the writer as a job: begun at one revision of the drawing, it writes as many records as each step is given — a dimension's anonymous block counts as one — and a change to the drawing meanwhile ends it, since what was written no longer describes the drawing. `export` is the job stepped to the end in one go, so DXFOUT is unchanged. Three coarse calls cross the boundary (`snapshot_begin`, `snapshot_step`, `snapshot_bytes`), and the app writes the autosave 2,000 records per turn of the event loop — about ten milliseconds of a large drawing — with frames drawn between; a change under it abandons the job, and the next pause keeps the drawing with the change. The snapshot is bytes, not a string: what the engine writes crosses the boundary once and goes into IndexedDB as it is.

## Alternatives considered

- **A worker with a replica engine.** The right shape if several things need the drawing off the main thread. Nothing does yet; see "Revisit when".
- **A worker parsing the DXF for the open.** The built drawing cannot be transferred, so the parse would be followed by a second build on the main thread, or by a serialisation format of our own. The open is a user action with a visible result; 0.45 s at 100k is within the budget.
- **A longer autosave delay at large sizes.** Fewer freezes, each as long. The freeze is what is wrong, not its frequency.
- **Incremental autosave (a change journal).** A second persistence format, to be replayed on recovery, for what slicing fixes with none.

## Evidence

- `crates/cad-dxf/tests/fixtures.rs`: every valid fixture exports the same in slices of three records; a change ends an export, and one begun afresh is the drawing as it is.
- `web/src/cad/engine.test.ts`: the snapshot in slices is the snapshot, taken once, and ends when the drawing changes.
- `web/src/app/cad-app.test.ts`: a change while the snapshot is being written abandons it, and the next pause keeps both; the autosave and recovery tests, with the record as bytes.
- `web/e2e/autosave.spec.ts`, `failure.spec.ts`, `files.spec.ts`: recovery as before.
- The measurements above.

## Consequences

- `cad_dxf::{Export, DrawingChanged}`; `Drawing::record`; `CadSession::{snapshot_begin, snapshot_step, snapshot_bytes}`; `CadEngine::{snapshotBegin, snapshotStep, snapshotBytes}`, `SnapshotStatus`; `SNAPSHOT_SLICE`; `AutosaveRecord.bytes` in place of `text`, with a record kept before this read as before.
- `pnpm --dir web bench:main-thread` measures the three costs, whole and sliced, on every bench run.

## Revisit when

- A second consumer of the drawing off the main thread appears — a search index, collaboration, a background DXF check — which is when a replica engine in a worker pays for itself.
- Opening at 100k crosses a second on the machines people use (#110), which a parse in a worker with a transferable built drawing would address.
