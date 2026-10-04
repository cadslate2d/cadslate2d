# M0 exit review — The Line

**Date:** 2026-10-04  
**Scope:** DESIGN §26–28  
**Outcome:** 18 of 18 acceptance criteria demonstrated by automated tests. The one check that needed a person — opening an exported file in a real CAD program — was done by the maintainer on 2026-10-04. M0 is closed.

M0 exists to prove the architecture with one entity. This review records the evidence, answers the questions §28 says M0 should answer, and lists what changes for M1.

## Acceptance criteria (§26.3)

Paths are relative to the repository root. "e2e" tests run in Chromium against the production bundle served by a plain static file server.

| # | Criterion | Evidence |
| --- | --- | --- |
| 1 | UI does not own drawing geometry | `UiState` carries prompt, log, cursor, counters and nothing else (`crates/cad-commands/src/ui_state.rs`). React reads one store (`web/src/app/ui-store.ts`). The exported WASM API is snapshot-tested and names no entity or vertex (`web/src/cad/render-boundary.test.ts`). |
| 2 | Rust owns the line | `Drawing` has no public mutators; writes go through `transaction`, `undo`, `redo` (`crates/cad-document/src/drawing.rs`, 17 tests). |
| 3 | TypeScript owns the WebGL2 context | `web/src/renderer/webgl/webgl-renderer.ts`. ESLint forbids WebGL outside `src/renderer/`; `scripts/check-boundaries.sh` forbids `web-sys`/`js-sys` outside `cad-wasm`, and `web-sys` is not used at all. |
| 4 | Rust produces batched `RenderScene` and `OverlayFrame`, not GL calls | `render_boundary.rs::frames_are_produced_in_bulk`; calls per frame are constant for 1, 100, and 2000 entities (`render-boundary.test.ts`). |
| 5 | Committed buffers are not regenerated or re-uploaded because the pointer moves | e2e `canvas.spec.ts` "pan, zoom, and pointer movement never re-upload committed geometry"; `render_boundary.rs::pointer_only_updates_do_not_change_the_scene_revision` (scene built once across 500 moves). |
| 6 | The LINE rubber-band updates only overlay data | e2e `line.spec.ts` "the rubber band updates only overlay buffers"; `overlay.rs::pointer_movement_changes_the_overlay_but_never_the_committed_scene`. |
| 7 | `L` and `LINE` invoke the same command | `cad-input` `resolve.rs::every_spelling_of_a_command_resolves_identically`; e2e "L and LINE start the same command". |
| 8 | Pointer and typed coordinates feed the same state machine | `session.rs::a_clicked_point_and_a_typed_point_are_equivalent`; e2e "pointer and typed points feed the same command". |
| 9 | The preview is transient | `commands/line.rs::pointer_movement_during_the_command_never_touches_the_document` (1000 moves, same revision, same next ID). |
| 10 | The committed line has a stable logical `EntityId` | `drawing.rs::undo_removes_a_created_entity_and_redo_restores_the_same_id`; `store.rs::ids_stay_valid_while_other_entities_are_removed_and_slots_reused`. |
| 11 | `next_entity_id` never decrements through undo | `drawing.rs::the_id_allocator_never_rolls_back_through_undo` and the property test over random edit/undo/redo sequences; `commands/history.rs::the_design_document_command_example`. |
| 12 | Selection uses an indexed candidate query | `tests/selection.rs`: a spy index sees exactly one pick-box query, and an index that proposes nothing yields no selection even with a line under the cursor. |
| 13 | Undo removes the line | `commands/history.rs`; e2e "Ctrl+Z and Ctrl+Y undo and redo" and the user story. |
| 14 | Redo restores it with the same identity | `drawing.rs`, `commands/history.rs::the_design_document_command_example`. |
| 15 | New geometry after undo discards the redo branch without reusing IDs | `drawing.rs::new_work_after_undo_discards_the_redo_branch_without_reusing_ids`; `commands/history.rs::new_geometry_after_undo_discards_the_redo_branch_without_reusing_ids`. |
| 16 | DXF export writes the line | `cad-dxf` `export.rs`, golden file `test-data/dxf/valid/cadslate-m0-line.dxf`; e2e `files.spec.ts` "DXFOUT downloads the drawing as DXF". |
| 17 | DXF import restores it | `cad-dxf` `import.rs` and `tests/fixtures.rs`; property test: export → import is bit-exact; e2e "OPEN restores a line from a DXF file". |
| 18 | Refreshing does not reveal a hidden server dependency | e2e `user-story.spec.ts` (every request is a same-origin `GET` for a static file) and `static-hosting.spec.ts` (works under a nested base path, and with the network switched off). |

The whole §26.1 user story runs as one e2e test, ending with a second export that is byte-identical to the first.

Test counts at this review: 264 Rust tests (unit, property, fixture, regression), 76 Vitest tests running the real WASM module, 39 Playwright tests.

## Architectural questions (§28)

**Is the WASM API coarse enough?** Yes. `CadSession` exports 22 calls; a steady-state frame makes four of them, whatever the drawing size. One extension was needed: a `HOST` dirty flag so file commands can ask the browser to act without Rust calling JavaScript (ADR-0002, amended).

**Is the `RenderScene` / `OverlayFrame` split natural in practice?** Yes. It falls exactly on the line between document and session: the scene is a function of `Drawing`, the overlay a function of session state plus the view. Selection highlight fits the overlay without strain. One nuance: the overlay depends on the view (the crosshair is sized in pixels) while the scene must not.

**Is the coarse render transport efficient enough?** Yes, with a wide margin. A pointer frame costs about 3 µs of main-thread time regardless of drawing size, and is indistinguishable from an empty GPU round trip; a committed edit at 100k lines costs about 2 ms including a full scene rebuild and re-upload (ADR-0008, corrected after its first measurement turned out to exclude GPU-side work).

**Is TypeScript-side WebGL ownership clean?** Yes. The adapter is about 300 lines, handles context loss by itself, and takes its colours from theme tokens through semantic style roles, so Rust knows no colours.

**Does pointer movement avoid committed-scene regeneration and upload?** Yes. Proven at three levels: the scene cache's build counter in Rust, the engine's revision in Vitest, and the renderer's upload counter in the browser.

**Is input state comfortable to model in Rust?** Yes. Commands as explicit state machines are short (LINE is about 60 lines) and fully testable without a browser. The one piece left in TypeScript, the text being typed, works well as a native input (ADR-0002 §6); Rust still decides what every submitted string means.

**Is dense-store + stable-ID separation practical?** Yes. About 100 lines, 1.4–1.8× faster to iterate than a hash map alone (ADR-0006).

**Is the transaction API natural?** Mostly. A builder that records changes and applies them on commit keeps "nothing changes until commit" trivially true. Allowing each entity to change at most once per transaction makes commit infallible. Open point for M1: undo granularity. LINE commits one transaction per segment, so one UNDO removes one segment; classic CAD removes the whole command's work.

**Does transient preview remain clean?** Yes. A command describes its preview semantically (`OverlayItem`); `cad-render` turns it into segments. No command builds vertex data, and no preview touches the document.

**Does selection/index synchronisation feel simple?** Yes. The drawing owns the index and updates it inside `apply`, the only function that mutates canonical state; a property test holds it equal to a rebuild after random edit sequences. Selection is pruned after every command step, so it can never name a dead entity.

**Is DXF translation cleanly isolated?** Yes. `cad-document` contains no DXF concept; `cad-dxf` depends only on the document; `cad-wasm` glues import to the session in ten lines.

**Does React remain free of CAD state duplication?** Yes. Components read one external store holding presentation state. Lint rules keep the WASM package and WebGL out of the UI layer.

**Do typed-array views and buffer reuse avoid unnecessary frame allocations?** Views cost 0.1–0.2 µs to create; the JS heap does not grow over 20,000 pointer frames; GPU buffers never reallocate once sized. Each pull does create a few short-lived small objects; the probe confirms they do not accumulate but cannot measure their rate.

## What M0 changed or resolved

| Item | Was | Now |
| --- | --- | --- |
| DXF release target (§24.1) | PROVISIONAL | R12 writer, release-agnostic reader — ADR-0005 |
| Entity store (§10.3) | PROVISIONAL | Dense vector + ID-to-slot map — ADR-0006 |
| Spatial index (§20) | PROVISIONAL | rstar R*-tree, insertion only; re-measure in M2 — ADR-0007 |
| Render buffer transport (§12.6) | PROVISIONAL | Views, instanced segments, reused buffers — ADR-0008 |
| View ownership | unspecified | Rust owns it, in CSS pixels — ADR-0003 |
| Curve-rendering experiment (§12.5 vs §23.3) | Two milestones named | M1, first issue of the curve epic |

No LOCKED decision was changed.

## Things the implementation taught us

- **Benchmarks at the stated scale targets find bugs that tests do not.** The 100,000-line benchmark crashed inside the R-tree library on the first run (ADR-0007). The fix is in place and pinned by regression tests.
- **Realistic fixtures matter.** A file written by a DXF library as R12 showed that R12 names the model-space block `$MODEL_SPACE`; the first reader reported it as user content.
- **Exhaustive matches over `Entity` are a feature.** The `#[non_exhaustive]` attribute suggested in the issue was removed: when circles arrive, the compiler should point at every place that must handle them.
- **`opt-level = 3` is worth 18 KB.** Importing 100k lines dropped from 610 ms to 235 ms.
- **Software WebGL is enough for CI.** The whole end-to-end suite runs headless in about ten seconds, which makes "prove it in the browser" cheap.

## Open items

1. ~~Open an exported DXF in a real CAD program~~ — done by the maintainer on 2026-10-04; the file loaded correctly. The maintainer also exercised the web UI by hand.
2. **GPU upload cost on real hardware** is unmeasured; the transport comparison ran on a software rasteriser (ADR-0008).
3. **Firefox and WebKit** are not part of the automated suite.
4. **Opening very large files** spends most of its time building the index by insertion, because bulk loading is unsafe (ADR-0007).
5. **The rstar bug has not been reported upstream.** That is a post to a third-party tracker and is left to the maintainer.

Items 2–4 are tracked as issues; none blocks M1.
