# M20 exit review — AI Foundations

**Date:** 2026-10-24  
**Scope:** natural language as one more, optional, way to drive CadSlate2D, asked for by the user on 2026-10-19 ([roadmap](../roadmap.md), epic #251, [ADR-0037](../adr/0037-ai-input-method.md)); the service in its own private repository, `cadslate2d/cadslate2d-ai-service`  
**Outcome:** Every item is implemented, measured, and tested. Two things need a person: a provider key for the nightly job, and the production deployment.

What M20 delivered:
- A model's output can only reach the drawing as a versioned, validated intent document (`cad-intent`). The document is compiled to registered commands, run through the one entry point typing uses, previewed on the canvas, and committed on Accept as one undo.
- The service runs, hosted: accounts by emailed link, the model adapter, metering against a monthly allowance, and records kept 30 days with drawing context never stored.
- The AI panel sits beside Layers and Properties. It sends drawing context only after the user agrees to that request, and lets the user accept, revise, or reject each proposal. Without the service the panel says why, and drafting is untouched.
- A corpus of 70 requests in English and Portuguese, each with the drawing it should make, is replayed from recordings in `make check`. The same corpus is asked of the model live: 70 of 70 passed on both live runs.

## The scenario

`web/e2e/m20-scenario.spec.ts`, in Chromium and Firefox, drafting by hand between every step:

1. **Sign in:** a link is emailed; the app opens at it, the token is spent, and the address bar loses it; the panel shows 3 of 3 actions left.
2. **Draw by a request:** "Draw a circle of radius 25 centred at 100,100" is proposed ("Adds 1 object"), the drawing unchanged until Accept, then one circle.
3. **Edit the selection by a request, with consent:** with everything selected, "Move these 25 mm up" first shows what would be sent — the line and the circle as the Properties panel describes them — and is sent on "Send with these".
4. **Preview, accept, undo:** "Changes 2 objects"; accepted, the saved DXF has both moved 25 up; one `U` puts both back.
5. **The limit:** a request the operations cannot do ("Extrude this into a 3D box…") is answered and counted; at 0 of 3 the prompt closes and says when the actions come back. A circle drawn by hand after that is drawn.
6. **Sign out:** the panel offers sign-in again; a line drawn after that is drawn.

It runs in CI against the tests' stand-in service (`web/e2e/ai-fake.ts`), which answers from the corpus's recordings of the real model. It also ran against the service itself, with the replay model, the mail sink, and an allowance of 3 (`cadslate2d-ai-service/scripts/app-scenario.sh`, which builds the app with the service's address and reads the link from the service's test mailbox). It passed in both browsers once the panel fix below was in. The issue asked for the staging deployment itself; that deployment asks the real model and sends real email, so the run used the staging build on this machine instead.

## Done when (epic #251)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An ADR amends DESIGN §3, §4, §43, §44, §45, and the deployment rule, without touching drafting being free, local, and account-free | #252 | [ADR-0037](../adr/0037-ai-input-method.md), accepted 2026-10-21; the amendments in DESIGN |
| A model's output becomes a drawing only through a validated IR compiled to registered commands, previewed, committed on Accept as one undo | #253, #254 | `cad-intent` (26 operations, `COMMANDS` the allowlist with no file, host, or settings command, the schema snapshot `test-data/intent/schema-v1.json`, the `intent_parse` fuzz target); `Session::run_intent` tests in `crates/cad-commands/src/tests/intent.rs`; `Session::propose` tests in `tests/proposal.rs` — the preview leaves the revision unchanged, Accept is one undo |
| The service runs, in its own repository: accounts, the model adapter, metering, what it keeps | service #1–#5 | The service's `tests/http.rs` (20 tests against Postgres) and `tests/model.rs` (7); its ADR-0001; a staging deployment; the privacy page at `/privacy` |
| The AI panel drives AI commands on the drawing and the selection, in any language, with per-request consent for context | #260, #261 | `web/src/app/ai-assistant.test.ts` (11 tests); `web/e2e/ai-panel.spec.ts` (8 tests, among them *a creation lands in the drawing as the model proposed it*, *an edit of the selection, by the IDs the context gave out*, *what the engine refuses is explained*, *drawing context is sent only when the user agrees*); the corpus in two languages |
| CI replays recorded responses; a nightly live evaluation reports success rates | #261, #262, service #12 | `scripts/check.sh` runs `ai-eval test-data/ai` (every case must pass); `crates/cad-commands/src/tests/corpus.rs`; the service's `.github/workflows/nightly-eval.yml` and `scripts/nightly-eval.sh` — waiting on its provider key, see below |
| A scenario and a review | #263 | `web/e2e/m20-scenario.spec.ts`; this review |

## Measurements

**The live evaluation**: the L1 corpus asked of the model through the staging service's adapter and instructions, then scored by `ai-eval` against the committed recordings as the baseline. This is the baseline the nightly job compares with:

| Level | Language | Recorded (committed) | Live, 2026-10-24 (first) | Live, 2026-10-24 (second) |
| --- | --- | --- | --- | --- |
| l1 | en | 35/35 | 35/35 | 35/35 |
| l1 | pt | 35/35 | 35/35 | 35/35 |

A case passes only when:
- the recorded document, run from the case's drawing, makes the expected drawing (geometry to a hundredth, layer, colour);
- a request the operations cannot do proposes nothing;
- a refusal is refused, or seen to be impossible, and explained;
- the reply is in the request's language.

**Cost per request:** measured in the service's repository, where the service's business terms are kept.

**Latency:** measured by the recorder for each request, eight at a time from the staging machine. It covers every attempt and, for a refusal, the explanation:

| | p50 | p90 | Range |
| --- | --- | --- | --- |
| Time to answer a request | 2.51 s | 4.25 s | 1.30 s – 14.77 s |

The slowest, 14.8 s, was a fillet the model judged impossible, followed by an explanation. Two other refusals took 5.8 s and 6.3 s.

The service adds little of its own:
- `/health` answers in 205 ms at p50 from here, which is network to `iad`;
- a stopped staging machine starts in 1.6–1.9 s (service ADR-0001).

**Size:** the panel is TypeScript only. The bundle's main script is 103.6 KB gzipped and the engine 1.07 MB gzipped. `cad-intent`'s schema generator stays out of the engine, behind a feature only the service uses.

**The suite:** 1,875 Rust tests, 190 web unit tests, and 549 end-to-end runs passing in Chromium and Firefox (580 listed; the visual baselines run in Chromium only). The service has 27 tests.

## Decisions

- **AI is an input method** (ADR-0037).
  - The model writes an intent document, never DXF or geometry. The document is validated against limits, compiled to commands from an allowlist, and run through `Session::submit_text`, exactly as a menu's macro is.
  - A proposal is that run on a clone of the drawing, drawn by the overlay. Accept runs it again on the drawing inside one undo group.
- **The service is private and elsewhere; the app does not need it.**
  - The address is a build setting (`VITE_AI_SERVICE_URL`); a build without one has no AI.
  - This repository's tests never reach the service: the e2e suite answers from a stand-in that follows its routes (the user's choice, option 1).
- **Hosting, mail, and the model** are the user's choices, recorded in the service's repository.
- **The model** answers with structured output shaped by the intent schema. An invalid document is sent back once with the reasons, then reported, never applied. The provider is asked not to store completions.
- **Consent per request.** Selected objects are described as the Properties panel describes them, with polylines' vertices, and only after the user sees the list and agrees. Without a selection a request goes alone. The service has no column for context.
- **Free allowance: 100 actions a month.** This was my default, not a decision the user made; it is a setting (`ALLOWANCES`) and is open to change.
- **A revision goes with the proposal it refines** (its request and document), and the answer replaces it.
- **A refusal is explained in the user's words.** When the engine stops a proposal, the service asks the model why, in the request's language, within the same action.
- **Evaluation:**
  - The corpus is judged by the drawing a document makes, not by the document's text: a triangle may be one polyline or three lines.
  - A regression is a language falling more than 5 points below the baseline (`REGRESSION_TOLERANCE`); one stray miss in 35 is not one.

## What testing found

- **An intent's `trim` and `extend` ignored the edges they named** (#261). TRIM clears the selection as it starts, so every object became an edge; a pick between two edges cut at the wrong one. A command that clears what was selected now gets it again at its own prompt. Tested with a case where the two answers differ.
- **Edits refused silently** (#261). OFFSET, TRIM, and EXTEND go on asking after a refusal, so a proposal that changed nothing looked like a success. A run that changes nothing and says why is now `RunError::Refused`, with the command's own sentence.
- **The model answered in the wrong language** (#261). The first recording of the corpus answered one English request in Russian and one in Spanish. The instructions now hold the reply to the request's language, and the scorer checks every reply and explanation.
- **The panel's exchange shrank to nothing** in a short panel beside a long email address (this scenario, against the service, in Firefox). The proposal and Accept were out of reach. The exchange now keeps room and the panel scrolls; `ai-panel.spec.ts` signs in with a long address to hold it.
- **CI drew text in colour** (subpixel antialiasing) where a developer machine draws it in grey, so the panel's baseline differed by a pixel. Chromium now runs with `--disable-lcd-text`.

## What needs a person

- **The nightly job's provider key.** `.github/workflows/nightly-eval.yml` in the service repository needs the model provider's key as a secret. A key of its own with a spending limit is best. Until it is set the job fails at its first step and says why. Service #12 stays open until its first night has reported.
- **Production.** The production deployment needs the app's public address for its allowed origin and its sign-in links, and a decision on the Free allowance.
- **The provider's zero-data-retention option** is an application only the account holder can make. Until then completions are sent with `store: false`.

## What follows

**M21 — Constraints and Constrained Generation** is next: geometric and dimensional constraints in the drawing, then the AI's second level, generation within constraints. Its issues are planned. **M22, the floor-plan generator,** builds on both. The live evaluation already scores by level, so L2 and L3 corpora add directories under `test-data/ai/`, and the nightly job picks them up.
