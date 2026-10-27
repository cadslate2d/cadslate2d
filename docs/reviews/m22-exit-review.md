# M22 exit review — Floor-Plan Generator

**Date:** 2026-10-26  
**Scope:** the first domain generator. A brief — a terrain, rooms with target areas, which rooms open onto each other, walls — becomes a plan of clean, editable walls, doors, labels, and areas. The plan comes from a deterministic solver, through a `FLOORPLAN` command and through the AI, with revisions in conversation. Asked for by the user on 2026-10-19 ([roadmap](../roadmap.md), epic #276, [ADR-0039](../adr/0039-layout-synthesis.md)).  
**Outcome:** Every item is implemented, measured, and tested. ADR-0039 was accepted *provisionally*: the solver was to be treated as production-ready only once this review confirmed the conditions the maintainer's review set. It does, below, with one brief at the edge of feasibility reported as such. Nothing in M22 waits on a person.

What M22 delivered:
- **`cad-layout`,** a pure crate on `cad-core`:
  - the brief;
  - a multi-start annealing search over slicing trees, ranked lexicographically;
  - refusals that tell a proof (`Infeasible`) from a search that found nothing (`Exhausted`);
  - an independent verifier;
  - the drafting of a plan: walls as the space between floors, doors after the topology, labels.
- **`FLOORPLAN`:**
  - the brief at the command line, as JSON, or from a spec file;
  - the three best plans previewed and cycled;
  - the one accepted drawn as one undo on `WALL-EXT`, `WALL-INT`, `DOOR`, and `ROOM-TEXT`;
  - started from Draw › Floor Plan....
- **Floor plans through the AI.** The intent IR (version 5) gains `floor_plan`. A revision — a room resized, rooms side by side, another option, other setbacks — is that operation with its brief revised, answered through the panel's Revise.

## The scenario

`web/e2e/m22-scenario.spec.ts` runs in Chromium and Firefox against the tests' stand-in service, which answers from the L3 corpus's recordings of the real model:

1. **A brief by a request:** "A two-bedroom house on a 12 × 20 m plot with a 5 m front setback: …" is proposed, and the drawing is untouched until Accept.
2. **A revision:** "Make the kitchen 14 m²" goes with the brief it revises.
3. **Another plan:** "Show me a different layout" goes with the kitchen's revision, which it keeps. Once accepted, the kitchen's label says 14 m².
4. **The walls edited by hand:** an interior wall face is chosen by a crossing polygon round its middle and moved with `MOVE`. It is an ordinary line.
5. **Dimensioned, saved, plotted:**
   - `DIMLINEAR` measures the building's front, the outer face's ends;
   - `SAVEAS` writes the DXF with the dimension and the plan's layers;
   - `PLOT` makes the PDF.

## Done when (epic #276)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| A layout solver meets the MVP's hard constraints, and reports infeasibility with reasons | #277, #278 | [ADR-0039](../adr/0039-layout-synthesis.md). `crates/cad-layout`: `verify`, independent of the search, with adversarial tests at every bound. The property tests over generated briefs. The seven necessary conditions, each tested a millionth either side of its bound. The corpus `test-data/layout/briefs.jsonl`, 32 briefs, held by `tests/briefs.rs`. The reliability study, below |
| Walls are drawn from shared and exterior boundaries at their thicknesses, with clean junctions; doors follow the topology; rooms are labelled with their areas | #279, #280 | `crates/cad-layout/src/tests.rs`: faces half a wall from each centreline; no overlap or gap at L, T, or X junctions, sampled every 7 cm; every room reached through doors; openings inside their walls and clear of junctions; jambs; leaves and swings. All held by hand and as a property over the search's plans. `crates/cad-commands/src/floorplan.rs` draws them as entities on their layers |
| FLOORPLAN works without AI and offers alternatives; the AI builds and revises specs conversationally | #281, #282 | `crates/cad-commands/src/tests/floorplan.rs`: briefs typed, as JSON, and from a file; plans cycled; refusals; the preview equal to the drawing. `web/e2e/floorplan.spec.ts`. The intent IR's `floor_plan` and its tests. The L3 corpus, and `ai-panel.spec.ts` › *a floor plan by a brief, a revision, and Accept* |
| A scenario and a review | #283 | `web/e2e/m22-scenario.spec.ts`; this review |

## The conditions of ADR-0039's acceptance

| The review asked for | Where it stands |
| --- | --- |
| Multi-start reliability per brief, from 100 or more starting seeds | `examples/reliability.rs`, the table in ADR-0039. Each of the 24 feasible briefs makes its plan from 100 starts of 100. The one brief at the edge (a hall that must open onto five rooms and the street within an aspect ratio of 2.5) makes it from 71. Every refusal is as expected. 3,171 of 3,200 runs in all. |
| Hard constraints lexicographic, not weighed | Only plans that keep every hard constraint are returned. They are ranked without circulation first, then by soft energy. The annealing's penalty weight shapes the walk, never the result (ADR-0039 §4). |
| Refusals that tell impossibility from search failure | `Refusal::{Invalid, Infeasible, Exhausted}`. `FLOORPLAN` says "This cannot be built: …" only for a proof, and the AI's explanations are told the difference. |
| An explicit circulation contract | `max_circulation`, 15% by default; 0 forbids it. It is added only after the rooms alone gave no plan, and it is named in the plan and labelled in the drawing. It appeared in 4 of the 25 feasible briefs, at most 13%. |
| An adversarial verifier | `verify` shares no geometry with the search. Hand-made plans break it at T-junctions, at corners, and a micrometre or a millimetre past every bound. |
| Determinism stated as far as it goes | Determinism holds in one build on one platform, and is tested. It is not claimed across platforms (`exp` and `powf`), so tests judge plans by `verify`, never by coordinates. |

## Measurements

**The solver** (ADR-0039, 100 starts per brief, native release):
- a plan in 11 ms at p50, 142 ms at p90;
- 318 ms at most for a feasible brief (the 20-room office floor);
- an exhausted refusal at the 24-room limit in 2.5 s;
- a proved refusal in microseconds.

**The AI's third level.** The L3 corpus is 42 floor-plan requests in English and Portuguese: 12 briefs, 7 revisions, and 2 refusals in each language.

| Level | Language | Recorded (committed) | Live, 2026-10-26 (run 40278681857) |
| --- | --- | --- | --- |
| l3 | en | 21/21 | 20/20 |
| l3 | pt | 21/21 | 20/20 |
| l2 | en / pt | 24/24, 24/24 | 24/24, 24/24 |
| l1 | en / pt | 35/35, 35/35 | 35/35, 35/35 |

The live run asked the 40 cases there were then; the two `different-layout` cases were added for this scenario and recorded after it. The first full recording under the final instructions made 19/20 in each language. One Portuguese case's wording ("nos fundos") did not say the storeroom opens onto the shop, and was made plain. The model answered one English refusal in Portuguese, 2 times in 9 asks. Both are recorded in the corpus README.

**Cost and latency of L3:**
- **Cost:** 40 requests spent 264,001 input tokens (251,555 cached) and 20,443 output: **$0.014, about $0.00035 a request**, three and a half times L1's. A brief is longer to write than a circle, and the output is most of the cost.
- **Latency:** p50 **11.3 s**, p90 14.5 s, from 6.8 s to 19.6 s, measured by the recorder from staging. A floor plan is the slowest request the AI answers. The panel shows it working; the solver's own share is under a tenth of a second.

**The suite:**
- 1,986 Rust tests;
- 192 web unit tests;
- 564 end-to-end runs passing in Chromium and Firefox (32 skipped, the visual baselines being Chromium's only);
- the service: 29 tests.

**Size:** the engine is 1.29 MB gzipped (M21: 1.23 MB); the main script 105.8 KB. `cad-layout` and the floor-plan drawing account for the growth.

## Commands added (registry sweep)

| Command | Alias | What |
| --- | --- | --- |
| `FLOORPLAN` | `FPLAN` | A floor plan from a brief: prompted, `Brief` (JSON), or `File`; `Next`, `Previous`, `Accept` |

Draw › Floor Plan... starts it. `menus.test.ts` runs its macro.

## Decisions

- **The solver is ours, and searches.** Its slicing trees can make only plans that are one rectangle cut into rectangles. That is the MVP, and ADR-0039 says what would make us revisit it.
- **The walls are the space between the floors.** A plan is drawn as each floor's sides and the building's outer face. Thicknesses come out exact and junctions clean without any wall-joining code, and every line is an ordinary entity (DESIGN §4).
- **Doors after the topology.** A door goes on each wall the search chose, at the middle of the stretch shared, opening into the room further from the entrance. The way in is on the entrance's front. Windows and furniture are left to later, as #280 said.
- **One entry point for briefs.** A spec file is typed into `FLOORPLAN`'s JSON prompt by the host, and so is the AI's `floor_plan`. The command line is the one way in, as ever (ADR-0034).
- **A revision is the brief revised, before Accept.** After Accept the plan is ordinary geometry, and the brief does not live on in the drawing. Revising an accepted plan means asking for a new one.
- **L3 is judged by what the brief must have.** A request in words leaves the areas to the model, so cases fix:
  - the plot;
  - rooms by kind and count;
  - totals;
  - pairings;
  - setbacks;
  - which plan;
  - for a revision, that the rest is kept.

  The plan must still be drawn. Each case's example answer is checked to meet its own expectation.

## What testing found

- **A 30/30 corpus result was not evidence of reliability.** From 100 starts, three briefs fell to 51–97%. Changing the penalty weight, the step count, and the budget, and searching with the rooms alone first, brought every feasible brief to 100 of 100 but the one at the edge (ADR-0039).
- **Circulation appeared where it was not needed,** in 71 of 100 one-bedroom plans. It is now added only after the rooms alone fail.
- **Asking for three plans tripled the time of an exhausted refusal.** Alternatives now cost more only once a plan exists.
- **A wall-overflow brief was refused only after two seconds of search.** A tighter necessary condition, (√floor + wall)² per room, proves it at once.
- **The stroke font had no `²`.** The labels drew "m□". `²` and `³` are now small raised digits.
- **The language check took a Portuguese explanation for Spanish** ("que" twice outweighed "as" and "e"). It took every reply with `Ø` for no language at all. Both are fixed, with tests.
- **The model made briefs that cannot be built.** It made a hall open onto every room, and forbade circulation unasked. The instructions now keep pairings to those asked for.
- **A property test of angular dimensions failed on a new random case.** The test judged a tangent line by its start, which is off the arc when the arrowheads are outside. The layout was right and the test was not. The test is corrected, and proptest keeps the case.
- **The layout crate's property tests ignored `PROPTEST_CASES`.** They fixed their own counts, so the stress run did not scale them. They now read it.

## What needs a person

Nothing new. Production (its address, and the Free allowance) and the provider's zero-data-retention application remain from M20.

## What follows

With M22 the milestones filed on 2026-10-18/19 are done in this repository:
- **M23 (AI plans and billing)** is the service repository's, but for the app's account view (#289);
- **the open-source release** waits on legal advice and a trademark (#292, #293).
