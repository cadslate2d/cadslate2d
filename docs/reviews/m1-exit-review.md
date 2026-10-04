# M1 exit review — Input and Basic Entities

**Date:** 2026-10-04  
**Scope:** DESIGN §29  
**Outcome:** The exit condition is met and demonstrated by an automated scenario. Every deliverable in §29 is implemented.

> **Exit condition (§29):** A user can create and edit a basic precise drawing primarily through keyboard and pointer interaction.

## The scenario

`web/e2e/drafting.spec.ts` drafts a small flange in the production bundle, in Chromium and Firefox:

1. `RECTANG`, `CIRCLE` (by radius and by `Diameter`), `ARC` (three points), `PLINE`, and `LINE` with relative coordinates — all from the keyboard.
2. `COPY` a bolt hole to three more positions (select first, then the command).
3. `MOVE` a line by an exact typed displacement.
4. `ERASE` an arc picked with a crossing box (command first, then select).
5. Three undos revert three whole commands; three redos restore them.
6. Export, refresh, reopen; the second export is byte-identical to the first.

Geometry is asserted exactly, through the exported DXF, not by eye.

## Deliverables (§29)

| Deliverable | Where | Notes |
| --- | --- | --- |
| Generalized command registry, aliases | `cad-commands/src/registry.rs` | Twelve commands registered as data; `Session::invoke` is the single entry point for typed input, shortcuts, and the toolbar |
| Parser diagnostics | `cad-input/src/resolve.rs` | Messages state everything the prompt accepts, generated from the same `ExpectedInput` that drives resolution |
| Bracketed keyword options | `cad-input/src/keyword.rs`, `Prompt::render` | A keyword wins inside a command; `C` is Close in LINE and CIRCLE at the idle prompt (§13.3); ambiguity is refused |
| Command/input history | `cad-commands/src/history.rs` | Up/Down recall; Enter at the idle prompt repeats the last command; scrollable log |
| Direct-distance entry | `Session::direct_distance_target` | A bare number travels towards the cursor from the command's base point |
| `LINE`, `CIRCLE`, `ARC`, `RECTANG`, basic `PLINE` | `cad-commands/src/commands/` | Each an explicit state machine with a transient preview |
| `ERASE`, `MOVE`, `COPY` | `commands/modify.rs` | Noun–verb and verb–noun; MOVE and COPY share one state machine |
| Selection windows | `Session::finish_selection_box` | Window (inside) and crossing (touching), by two clicks or a drag; Shift removes |
| Transient-preview framework | `cad-commands/src/transient.rs` | Previews are overlay descriptions; commit is one transaction; sources are exposed for snap exclusion |
| Status toggles framework | `cad-commands/src/toggles.rs` | OSNAP, GRID, ORTHO, POLAR flip from the status bar or F3/F7/F8/F10 without disturbing a command. Behaviours are M2 |

Beyond the list: DXF for circles, arcs, and polylines (so M1 drawings can be saved), a toolbar, and the end-to-end suite in a second browser engine.

Test counts at this review: 426 Rust tests, 82 Vitest tests, 73 Playwright tests per browser.

## Decisions made in M1

| Item | Decision |
| --- | --- |
| Curve rendering (§12.5, §23.3, EXPERIMENT) | Analytic arc instances under a covering strip — [ADR-0009](../adr/0009-curve-rendering.md) |
| Undo granularity | One command invocation is one undo step, through undo groups in the history; the in-command `Undo` rolls back without becoming redoable |
| Click selection | Clicks accumulate and Shift removes (classic behaviour), replacing M0's click-replaces |
| Polyline joins | Round, from the caps of adjacent segments. Miter joins with a limit wait for lineweights |
| DXF polylines | Written as R12 `POLYLINE`/`VERTEX`; `LWPOLYLINE` is read. Arc segments are reported, not flattened |
| Unit suffixes | Deferred to drawing settings: there is no drawing unit to convert against yet |

## What M1 taught us

- **A measurement was wrong, and the experiment that followed caught it.** `gl.finish()` returns in Chromium before queued GPU work has run. The first transport table in ADR-0008 therefore covered main-thread time only. The curve probe stalled for minutes when it finally forced those queued draws, which exposed the problem. Both probes now wait with a pixel read-back; ADR-0008 was corrected the same day. Its conclusions held; the gaps shrank from about 10× to about 5×.
- **Analytic curves have a measured precision limit.** Exact up to an on-screen radius of about 10⁵ pixels, visibly heavier from 3×10⁵, gone near 10⁷ (ADR-0009). Zooming into a tangency on a large arc will reach it.
- **Exhaustive matches paid off twice.** Adding `Circle`/`Arc` and then `Polyline` to `Entity` made the compiler list every place to update: rendering, picking, DXF export, test helpers.
- **Realistic fixtures keep finding things.** Files written by an independent DXF library confirmed the arc angle convention and both polyline encodings.
- **The second browser engine was cheap locally and not in CI.** All 73 end-to-end tests pass in Firefox on a developer machine without changes. On the hosted CI runner, headless Firefox provides no WebGL2, so CI still runs Chromium only; WebKit has not been run at all.

## Performance sanity check

Ten thousand lines (generated drawing), production bundle, headless Chromium with software GL: import 21 ms; a committed edit including full scene rebuild and re-upload 0.5 ms; a pointer-move frame indistinguishable from an empty GPU round trip (ADR-0008). Ten thousand arcs draw in 0.1–0.2 s per frame under software rasterisation (ADR-0009), which says little about real hardware.

**Not established:** frame time on a real GPU, for lines or curves. This needs a person with hardware and is tracked.

## Open items carried forward

1. Render measurements on real GPU hardware (needs a person).
1. Firefox in CI and any WebKit run at all (the cross-browser issue stays open).
2. Whether to report the R-tree library's bulk-load crash upstream (maintainer's call).
3. A fallback for curves at extreme zoom.
4. Unit suffixes and drawing units.
5. A safe bulk loader for the spatial index.
6. Miter joins, when lineweights exist.

## Adjustments for M2

- **Intersections.** `cad-geometry/src/crossing.rs` already has boolean box tests and segment–circle root finding. The M2 primitives extend this to returning intersection *points* and add the robust orientation predicate.
- **Transforms.** `Entity::translated` covers MOVE and COPY. ROTATE, SCALE, and MIRROR need a general similarity transform on every entity kind; mirroring reverses an arc's direction, which must be re-expressed counter-clockwise.
- **Snap exclusion.** `Command::transient_sources` already reports the entities a transient edit is moving (§18.2).
- **Toggles.** State, status-bar buttons, and function keys exist; M2 gives ORTHO, GRID, POLAR, and OSNAP their behaviour.
- **Grips.** Selection highlight and previews already go through the overlay; grips are another overlay item plus hit-testing priority.
- **Scope question still open:** §18 lists perpendicular and tangent snaps as initial modes, §30 leaves them out of M2. Recommendation: keep M2 as §30 lists it and take those two into M3, where the robust tangent and perpendicular constructions belong.
