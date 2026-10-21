# ADR-0037: Natural language is an optional input method, through a hosted service, a validated intent IR, and a previewed proposal

**Status:** Proposed (drafted 2026-10-21; records the product decisions the user made on 2026-10-19 and awaits the maintainer's acceptance, #252)  
**Date:** 2026-10-21  
**Kind:** Product and architecture (amends DESIGN §3, §4, §8, §43, §44, §45, §46; the rule of ADR-0034 applied to a model)  
**Design refs:** §3, §4, §8, §15, §43, §44, §45, §46

## Context

CadSlate2D is a drafting workstation delivered as a website: free, local-first, account-free, with no required backend (DESIGN §3, §8). DESIGN rules out much of what AI would bring: "AI-first interaction" (§4), "AI chat sidebars" and "mandatory accounts" (§45), "no drawing upload" and "no drawing-content telemetry" (§43), "free to use" (§44), "no required backend" (§8). §46 says such constraints are not reopened without an ADR.

On 2026-10-19 the user asked for natural language as a way to drive the program, and decided its shape in a design discussion (epic #251): AI is an input method and not a design mode; a model never writes DXF or moves a vertex; the service is hosted and lives in its own private repository; accounts are for AI only; the prompt is sent, drawing context only after the user confirms that request; the panel is the one sidebar; without the service everything else works. Later milestones stand on it: constraints and constrained generation (M21), a floor-plan generator (M22), plans and billing (M23).

This ADR records those decisions, draws the line they leave untouched, and sets the one architecture rule that makes the rest safe.

## Decision

1. **AI is one more input method.** Mouse, keyboard, typed commands, menu macros — and natural language. A request goes:

   ```text
   natural language → the service interprets intent → an intent IR, validated → the command engine → a previewed proposal → Accept → an editable drawing → DXF or DWG
   ```

   The model's output is a program in a small language (`cad-intent`, #253): operations from a catalogue — draw, modify, layers and properties, dimension — with references to existing entities by the stable IDs the context gave out. The engine validates it against a versioned schema and limits, compiles each operation into a registered command and its answers, and runs them through `Session`'s one entry point (DESIGN §15), exactly as a menu macro is run (ADR-0034). File, host, and settings commands are not in the catalogue. **No model output reaches the document except as a validated IR compiled to registered commands.**

2. **Every result is a proposal** (#254). The IR runs against a scratch copy; the overlay draws what it would add, change, and remove; the document and its revision are untouched until the user accepts, and then the proposal is one transaction and one undo. Reject leaves no trace. This is the rule that transient edits never touch the document, applied to a model.

3. **The service is hosted, optional, and elsewhere.** It lives in a private repository, `cadslate2d/cadslate2d-ai-service`: accounts, the model adapter, metering, what it keeps, the plans. This repository holds everything the app does with it: the IR, the panel, the constraints, the generators. A static build with no service configured has no AI at all, and the program does not change for it. Signed out, over the limit, or offline, the panel says why and everything else works.

4. **The boundary that stays.** Drafting, files, plotting, the generators as commands, and everything DESIGN §47 calls a successful v1 are free, local, account-free, and need no network. AI is off until the user opens the AI panel and signs in. An account is for AI only, and paying is for AI only.

5. **Privacy, per request.** The prompt is sent. Drawing context — the selection summary, the layers, what the request names — is sent only when a request needs it and only after the user confirms that request, shown what would go. Nothing is sent in the background. What the service keeps of a request is bounded and documented in its repository; drawing geometry is never stored there.

6. **The panel.** A dedicated AI panel beside Layers and Properties (#260) is the one sanctioned "AI sidebar": the prompt, the exchange, the proposal's steps, Accept, Revise, Reject, and the consent for context. Any language is understood, and replies come in the prompt's language.

7. **Quality is measured.** CI here replays recorded model outputs through the IR and the engine; the live evaluation against the model runs from the service repository, nightly, and reports success rates (#262).

## Alternatives considered

- **No AI.** DESIGN's default, and the product works without it. Rejected only because the user asked, and because the shape chosen keeps every property DESIGN protects: the drawing stays local, the drafting stays free, and the model stays outside the document.
- **AI as a design mode** — the model producing geometry, DXF, or edits directly. Rejected: invalid geometry, arbitrary actions, and no undo. The IR compiled to registered commands is the same single-entry-point rule the menus use, and the preview is the same rule transient edits use.
- **A model in the browser.** No account, no network, no service. Rejected for now: a model that reads intent well is far larger than the download budget (DESIGN §42), and the request still needs the engine's validation. The IR and the panel do not care where the model runs; a local model is an adapter, should one fit.
- **Accounts for the whole product**, or a "free tier" of drafting. Rejected: §45's mandatory accounts and account-gated drafting (§4) stay ruled out. The account is a key to one input method.
- **Sending drawing context with every request**, so the model always knows the drawing. Rejected: §43's "no drawing upload" becomes "no drawing upload the user did not confirm, request by request", and no further.
- **The service in this repository.** Rejected: it is a different work with different concerns — accounts, billing, what is kept — and ADR-0035's licence leaves it a separate, private work; the app works without it.

## Consequences

- **DESIGN is amended in this commit:** §3 (the wedge keeps "no mandatory account", with AI as the one optional, hosted, accountable input method); §4 ("AI-first interaction" stays ruled out; AI as an input method is not that); §8 (static hosting and no required backend stay; an optional hosted service is admitted for AI only); §43 (the default stays; what a confirmed AI request sends is stated); §44 (free to use stays for the product; paying is for AI only); §45 ("AI chat sidebars" become the one AI panel; "mandatory accounts" stay resisted); §46 lists this ADR. AGENTS.md gains the rule in a line and keeps the product constraints.
- **The roadmap** carries M20–M23 as planned on 2026-10-19.
- **New crates and calls:** `cad-intent` (pure Rust, placed by `scripts/check-boundaries.sh`); coarse WASM calls to propose, accept, reject, and summarise a proposal, each justified in the API snapshot test (ADR-0002).
- **New tests:** per-operation compile tests, a property test that any valid IR runs without a panic, a fuzz target on the IR parser, a snapshot of the schema, recorded replays in CI, e2e flows with a fake model, and an e2e test that context is never sent without consent.
- **What does not change:** everything else. A user who never opens the AI panel sees one more panel in the View menu and nothing more.

## Revisit when

- A model small enough for the download budget reads intent well: a local adapter would remove the account and the network for AI too.
- The service's retention or the context it needs changes: the privacy statement in §43 and the consent in the panel change with it, by ADR.
- The IR grows past the catalogue — constraints (M21), generators (M22) — which is the plan, and bumps the schema's version each time.
