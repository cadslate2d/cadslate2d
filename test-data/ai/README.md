# The AI corpus

Plain-language requests, each with what should come of it, and the AI model's recorded answers (ADR-0037, #261, #262). One directory per level; each holds `cases.jsonl`, `requests.jsonl`, and `recordings.jsonl`.

## `l1/`: AI commands (#261)

Level 1 is ordinary drafting said in words: new geometry, edits of the selection, revisions of a proposal, what the operations cannot do, and what the engine refuses. Each request is written in English and in Portuguese.

- **`cases.jsonl`** is written by hand, one case per line:
  - `name` (`circle-en`), `lang` (`en` or `pt`, the language the reply must be in), `prompt`;
  - `setup`, intent operations that make the drawing the request is made over (in millimetres), and `select`, the IDs selected;
  - `previous`, the request and document a revision replaces;
  - and what should come of it: `expected`, a list of documents any of which makes the drawing wanted (a triangle may be one polyline or three lines); `"expected": "nothing"`, for a request the operations cannot do; or `refused`, a document the engine refuses, whose reason the reply must explain.
- **`requests.jsonl`** is generated from the cases: what the app would send for each, its context from `Session::ai_context`, and the engine's reason for a refusal. `UPDATE_SNAPSHOTS=1 cargo test -p cad-commands corpus` writes it; the test fails when it is out of date.
- **`recordings.jsonl`** is the model's answers to those requests, written by the AI service's recorder: one line per request, the provider's attempts and, for a refusal, its explanation.

## Scoring

`cad_commands::evaluation` judges a recording against its case: the recorded document is run from the case's drawing, and the drawing it makes compared, object by object, with the expected one — geometry to a hundredth, layer, and colour; a polyline whichever vertex and way it is drawn from; a dimension by what it measures. A request that cannot be done must propose nothing; a refusal must be refused, or seen as impossible, and explained; every reply must be in the request's language (told by its common words).

- **In CI** the committed recordings are replayed: `scripts/check.sh` runs `ai-eval test-data/ai`, which prints the rate per level and language and fails on any miss, and `crates/cad-commands/src/tests/corpus.rs` holds the same and that the requests are current. Deterministic and free. The e2e stand-in service (`web/e2e/ai-fake.ts`) answers from the same recordings.
- **Live, nightly**, the AI service's repository asks the model every request again and scores the night's answers with `ai-eval test-data/ai --recordings DIR`, the committed recordings as the baseline: a language whose rate falls more than 5 points below the baseline's is a regression (`REGRESSION_TOLERANCE`), reported there.

| Level | Language | Baseline (2026-10-24, gpt-6-luna) |
| --- | --- | --- |
| l1 | en | 35/35 |
| l1 | pt | 35/35 |

## Recording again

Re-recording is deliberate: after changing the cases, the requests, or the service's instructions, write the requests (`UPDATE_SNAPSHOTS=1 cargo test -p cad-commands corpus`), deploy the service's staging, record with its `scripts/record.sh test-data/ai/l1/requests.jsonl > test-data/ai/l1/recordings.jsonl`, run `ai-eval`, and review the diff before committing it. The new recordings are the new baseline.
