# The AI corpus

Plain-language requests, each with what should come of it, and the AI model's recorded answers (ADR-0037).

## `l1/`: AI commands (#261)

Level 1 is ordinary drafting said in words: new geometry, edits of the selection, revisions of a proposal, what the operations cannot do, and what the engine refuses. Each request is written in English and in Portuguese.

- **`cases.jsonl`** is written by hand, one case per line:
  - `name` (`circle-en`), `lang` (`en` or `pt`, the language the reply must be in), `prompt`;
  - `setup`, intent operations that make the drawing the request is made over (in millimetres), and `select`, the IDs selected;
  - `previous`, the request and document a revision replaces;
  - and what should come of it: `expected`, a list of documents any of which makes the drawing wanted (a triangle may be one polyline or three lines); `"expected": "nothing"`, for a request the operations cannot do; or `refused`, a document the engine refuses, whose reason the reply must explain.
- **`requests.jsonl`** is generated from the cases: what the app would send for each, its context from `Session::ai_context`, and the engine's reason for a refusal. `UPDATE_SNAPSHOTS=1 cargo test -p cad-commands corpus` writes it; the test fails when it is out of date.
- **`recordings.jsonl`** is the model's answers to those requests, written by the AI service's recorder (`cadslate2d-ai-service`, `scripts/record.sh test-data/ai/l1/requests.jsonl`): one line per request, the provider's attempts and, for a refusal, its explanation.

`crates/cad-commands/src/tests/corpus.rs` replays the recordings: each recorded document is run from its case's drawing, and the drawing it makes compared, object by object, with the expected one — geometry to a hundredth, layer, and colour; a polyline whichever vertex and way it is drawn from; a dimension by what it measures. A request that cannot be done must propose nothing; a refusal must be refused, or seen as impossible, and explained; every reply must be in the request's language. The e2e stand-in service (`web/e2e/ai-fake.ts`) answers from the same recordings.

To record again after changing the cases, the requests, or the service's instructions: write the requests, deploy the service's staging, record, review the diff, and commit it.
