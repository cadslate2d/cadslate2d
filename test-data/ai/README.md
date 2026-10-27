# The AI corpus

Plain-language requests, each with what should come of it, and the AI model's recorded answers (ADR-0037, #261, #262). One directory per level; each holds `cases.jsonl`, `requests.jsonl`, and `recordings.jsonl`.

## `l1/`: AI commands (#261)

Level 1 is ordinary drafting said in words: new geometry, edits of the selection, revisions of a proposal, what the operations cannot do, and what the engine refuses. Each request is written in English and in Portuguese.

- **`cases.jsonl`** is written by hand, one case per line:
  - `name` (`circle-en`), `lang` (`en` or `pt`, the language the reply must be in), `prompt`;
  - `setup`, intent operations that make the drawing the request is made over (in millimetres), and `select`, the IDs selected;
  - `previous`, the request and document a revision replaces;
  - and what should come of it: `expected`, a list of documents any of which makes the drawing wanted (a triangle may be one polyline or three lines); `"expected": "nothing"`, for a request the operations cannot do; or `refused`, a document the engine refuses, whose reason the reply must explain.
  - `options`, for a request that asks for options (AI service #10): how many may come, `3` or `[2, 4]`. Each option is judged as a single answer would be, and no two may draw the same; a case without it wants no options, and an answer that offers them unasked is a miss. A floor plan's options are its brief's plans, and the corpus checks the example's brief has that many, each drawn differently.
- **`requests.jsonl`** is generated from the cases: what the app would send for each, its context from `Session::ai_context`, and the engine's reason for a refusal. `UPDATE_SNAPSHOTS=1 cargo test -p cad-commands corpus` writes it; the test fails when it is out of date.
- **`recordings.jsonl`** is the model's answers to those requests, written by the AI service's recorder: one line per request, the provider's attempts and, for a refusal, its explanation.

## `l2/`: constrained generation (#274)

Level 2 asks for parts — plates, slots, hole patterns, bolt circles, flanges, stairs — placements against what is drawn — tangent circles and arcs, concentric circles, arrays along a path and over a boundary — constraints, and parametric edits: "make this plate 20 mm wider without moving the holes relative to the edges", answered by setting the part's width parameter, which the context lists with the selected objects (`d1 = 120: horizontal distance, id 4 end to id 2 start`). Among them are edits the constraints cannot take, which the engine refuses with the solver's reason and the model explains. The cases are written as `l1`'s are; a drawing is compared with its constraints, each by kind or by name and value.

## `l3/`: floor plans (#282)

Level 3 asks for floor plans in words: "a 10 × 25 m plot, 3 bedrooms, one a suite, the kitchen connected to the living room, about 120 m²", then revisions of the plan before it is accepted ("make the kitchen 14 m²", "put the bathroom next to both bedrooms", "show me another option"), and plans that cannot be built, which the generator refuses — "This cannot be built" only when proved (ADR-0039) — and the model explains with the choices left. A request in words leaves the areas to the model, so a case says what the brief must have rather than the drawing: `plan` lists the plot (`terrain`, to 1%), rooms by kind and how many (`rooms`, patterns like `bedroom|quarto!bath`), how many in all (`count`), their total area (`total`), a room's area (`areas`), pairings (`adjacent`, and how many: `pairs`), `setbacks`, the room that holds the stair (`stair`, a pattern; the stair must be drawn) and the storey's height (`storey_height`), which plan (`alternative`), and for a revision that the rest of the brief is kept (`keeps_previous`); and the plan must be drawn. Each case's `example` is an answer that has them, which the corpus test checks.

## Scoring

`cad_commands::evaluation` judges a recording against its case: the recorded document is run from the case's drawing, and the drawing it makes compared, object by object, with the expected one — geometry to a hundredth, layer, and colour; a polyline whichever vertex and way it is drawn from; a dimension by what it measures. A request that cannot be done must propose nothing; a refusal must be refused, or seen as impossible, and explained; every reply must be in the request's language (told by its common words).

- **In CI** the committed recordings are replayed: `scripts/check.sh` runs `ai-eval test-data/ai`, which prints the rate per level and language and fails on any miss, and `crates/cad-commands/src/tests/corpus.rs` holds the same and that the requests are current. Deterministic and free. The e2e stand-in service (`web/e2e/ai-fake.ts`) answers from the same recordings.
- **Live, nightly**, the AI service's repository asks the model every request again and scores the night's answers with `ai-eval test-data/ai --recordings DIR`, the committed recordings as the baseline: a language whose rate falls more than 5 points below the baseline's is a regression (`REGRESSION_TOLERANCE`), reported there.

| Level | Language | Baseline (2026-10-27, gpt-6-luna, intent version 10) |
| --- | --- | --- |
| l1 | en | 37/37 |
| l1 | pt | 37/37 |
| l2 | en | 29/29 |
| l2 | pt | 29/29 |
| l3 | en | 26/26 |
| l3 | pt | 26/26 |

Recorded again on 2026-10-27 at intent version 10, after the instructions gained stairs (#331): a straight or U stair in plan and a stair's section by request, and a two-storey house whose stair hall holds the stair, in both languages, all made right. One English refusal, `too-big-en`, was explained in Portuguese; asked again it answered in English twice in three, and the committed recording is a re-ask. Before that, recorded on 2026-10-27 after the instructions gained trusses (#328): a Warren truss drawn from its description in both languages. The first run started the zigzag at each support, laying the end diagonals on the top chords; with a worked example in the instructions both languages draw it right, asked four times. One English refusal, `fillet-parallel-en`, was explained in Portuguese; asked again it answered in English three times in three, and the committed recording is a re-ask. Before that, on 2026-10-26 at intent version 9, after the instructions gained `members` (#329) and said that a plan group is only for a plan already drawn — the first run at version 9 answered a proposal's revision, `laundry-en`, by asking for the group. Every level at 100% but `members-pt`, which once put a top chord among the diagonals; asked again it grouped them right three times in three (four in five over both runs), and the committed recording is a re-ask. Before that, at intent version 8, after the instructions gained `dim_scale` (#327): every level at 100% but one Portuguese L2 case, `bigger-holes-pt`, which scaled the holes instead of setting their diameter parameter; asked again three times it set the parameter twice, and the committed recording is a re-ask. The new L1 cases dimension a 1200-unit line legibly, its scale set first. Before that, at intent version 7, after the context gained the selected objects' groups and the instructions how to revise a plan already drawn (#325): every level at 100%, among them windows added to an accepted plan — erased and planned again from its group's brief — in both languages. Before that, at intent version 6, after the instructions gained windows (#324): every level at 100%, among them a plan asked for with 150 cm windows and windows added to a proposed plan, in both languages. Before that, the same day, after the reply gained `alternatives` and the instructions options (AI service #10): every level at 100%, the new option cases — a plate's corners, a bolt circle's pitch, a house's three plans — among them, and no answer offering options unasked. Before that, the same day, at intent version 5, after the instructions gained floor plans; L1 accepts a circle drawn with an inscribed hexagon. The full run of L3 under the final instructions made 19/20 in each language: a Portuguese case whose wording ("nos fundos") did not say the storeroom opens onto the shop, since made plain in both languages, and an English refusal the model answered in Portuguese — asked again, it answered in English seven times in nine. The committed recordings re-ask those two. Earlier: recorded again on 2026-10-25 after the service's instructions gained the parts, placements, and constraints: the model now answers L1's plate and bolt circle with the parts, which those cases accept as well.

## Recording again

Re-recording is deliberate: after changing the cases, the requests, or the service's instructions, write the requests (`UPDATE_SNAPSHOTS=1 cargo test -p cad-commands corpus`), deploy the service's staging, record each level with its `scripts/record.sh test-data/ai/<level>/requests.jsonl > test-data/ai/<level>/recordings.jsonl`, run `ai-eval`, and review the diff before committing it. The new recordings are the new baseline.
