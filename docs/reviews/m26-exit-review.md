# M26 exit review — Checked Answers

**Date:** 2026-11-24  
**Scope:** an AI answer held to the request it answers and shown with how it was checked, after StructureClaw's traceable, artifact-checked workflow (arXiv 2607.14896), whose finding is that fluent output hides an incomplete result ([roadmap](../roadmap.md), milestone M26, [ADR-0037](../adr/0037-ai-input-method.md)).  
**Outcome:** Every item is implemented and tested, and the four issues are closed. Measured live, the packs and the requirement check cost nothing a night shows, and the packs make an everyday answer about a third faster. Three things need a person:
- #362 is closed with its title and body replaced by "x", which reads as done by mistake; its work is in the commits and the ADR (below);
- a check of what a sheet states is only as good as the requirements the model writes, which nothing checks against the request itself;
- the measurements are of one or two live runs an arm.

What M26 delivered:
- **Requirements checked** (#362): beside its document the model writes the request's measurable requirements, each in the request's words with a check (`cad_intent::Requirement`):
  - a view by id at a scale;
  - a dimension reading a value;
  - a text;
  - a count of closed outlines of a size;
  - a layer with something on it.

  The engine checks each on the document drawn as the review draws it (`cad_commands::requirements`), of a sheet alone, and only a dimension a request asks to be dimensioned. An unmet requirement goes back to the model with what the drawing has instead.
- **The trail** (#363): under each proposal, collapsed: what was read from the request of its sheet, each requirement met or not, why each answer after the first was asked for, and where the answer still departs — "How this was checked — 4 of 5 requirements met · asked 2 times".
- **Domain packs** (#364): the instructions are a core every request gets and packs chosen per request — trusses and members, isometric drawing, parts and constraints, buildings, drawing sheets. A request gets a pack by its words and those of the request it revises, a paper or scale it states, the operations of the documents it follows, and its context's groups and parameters (the service's `packs_for`).
- **Questions** (#365): a request that cannot be drawn as stated is answered with no document and a reply saying what conflicts or is missing. Such requests include a sheet and scale too small for it, sizes that contradict each other, a size it depends on and does not give, and calculation rather than drafting. The corpus gains a level, **L6**, judged by the words the reply must say (`Expected::Asks`).
- **A sheet held to its request** (with #358 of M25): what a request states of its sheet — paper, orientation, scales — is read from its words (`cad_commands::stated`) and never advised away. A revision is compared view by view with what it revises (`cad_commands::compare`).
- **The nightly made honest** (the service's #15):
  - A case the night misses is asked again, up to three asks, and passes on any.
  - The review gained three findings: a dimension with an end on nothing its view draws, a free view looking from a side or behind not as tall or wide as the principal view, and how narrow a sheet's notes must be to compose.

## The scenario

`web/e2e/m26-scenario.spec.ts` runs in Chromium and Firefox. Its requests are answered by the tests' stand-in service from the corpus's recordings of the real model:
1. **A contradiction said**: "Draw a rectangle 100 wide and 60 high whose diagonal is 150." The reply says the diagonal is 116.62. No proposal is made, and the drawing is unchanged.
2. **A sheet checked**: L5's bracket on A4 at 1:1. Its proposal shows how it was checked: "2 of 2 requirements met · asked once", A4 and 1:1 read from the request, each requirement ticked. The trail is the service's, added to the recorded answer as the service sends it. The proposal is accepted as one.
3. **One undo**: `U` takes the whole sheet away and `REDO` brings it back.

## Done when (M26)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An answer's requirements written and checked item by item | #362 | `cad_commands::requirements`: each requirement met and unmet, saying what was found. The service sends an unmet one back |
| How an answer was checked, shown with it | #363 | `AiPanel`'s trail; `ai-panel.spec.ts` "a proposal says how it was checked"; the M26 scenario |
| Packs in the service, the choice tested on the corpus's requests | #364 | The service's `every_corpus_request_is_given_the_packs_its_answer_needs` |
| The corpus at its targets with packs; the nightly's cost and score compared before and after | #364 | Live arms below, recorded in ADR-0037's as-built; the nightly at 228/228 ([run 45465046533](https://github.com/cadslate2d/cadslate2d-ai-service/actions/runs/45465046533)) |
| Requests whose right answer is a question, in the corpus | #365 | `test-data/ai/l6`: 4/4 in each language. `a_case_that_asks_is_met_by_saying_what_is_missing_and_drawing_nothing` |
| A scenario and a review | — | `web/e2e/m26-scenario.spec.ts`; this review |

## Measurements

**Packs and the requirement check, live.** Three arms of the whole corpus on the service of 0.2.24, each asked once, run one at a time. Three run together met the provider's rate limit and were discarded.

| Arm | Passed, first ask | A night | Median answer, l1 / l2 / l3 / l4 / l5 |
| --- | --- | --- | --- |
| As deployed: the packs a request is given (2 runs) | 449/456, 98.5% | $0.155 | 3.1 / 3.0 / 7.2 / 3.2 / 73 s |
| Every pack with every request: the one prompt of before | 225/228, 98.7% | $0.161 | 5.0 / 4.4 / 9.2 / 4.8 / 81 s |
| Unmet requirements asking nothing again | 226/228, 99.1% | $0.157 | 2.8 / 2.8 / 7.4 / 3.5 / 90 s |

- **The score is the same in every arm**, within the two or three stray misses any run makes.
- **So is the cost.** The instructions are read from the provider's cache at a tenth of the price, so the 4,600 tokens every pack adds to a request cost almost nothing.
- **What the packs change is time:** a third off an everyday answer.
- **The requirement check acts on sheets alone.** With it the eight sheets passed 16 of 16 over two runs, without it 7 of 8. That is too few to call a gain, and shows no cost. Both stay.

**Before and after, over the nightly's history.** At the first ask, the nights after 0.2.22 (2–24 November, 4,839 answers) passed 98.0% and cost $0.00080 a request. The nights before 0.2.20 (27–29 October, 641 answers) passed 99.1% and cost $0.00046 a request. The rise comes from two things, not the packs, as the arms show:
- a floor plan takes 1.54 model calls where it took 1.02. Of the 16 that took more than one in the deployed arm, 14 were revisions, which the revision check of #358 asks again;
- a sheet's answers are longer.

**The nightly** asks a miss again since the service's #15. Its first run with that passed 228/228, every level at 100%, four cases on their second ask.

## The whole suite

`make check` passes. The M26 and M25 scenarios pass in Chromium and in Firefox.

## Registry sweep

M26 adds no command. It adds:
- **To the intent:** the answer's `requirements` (`cad_intent::Requirement`), beside its document, not in it; intent version 26.
- **To the service's answer:** `trail`, which the panel shows.
- **To the corpus:** `Expected::Asks`, a case judged by what its reply says.

## What it does not do, and why

- **The requirements are the model's reading of the request.** A requirement it leaves out is not checked, and nothing compares the list with the request's words. What a request states of its sheet is read by the engine (`cad_commands::stated`) and held to apart from the list.
- **Only a sheet's requirements are checked.** A plain drawing's sizes are what it is drawn at, and checking them asked the model for dimensions nobody wanted (0.2.21, ADR-0037).
- **A pack is chosen by words and context, not by the model.** A request about trusses that never says "truss" or a word of the pack gets the core alone, which still holds every operation's schema.
- **Packs are by kind of drawing, not by trade.** The issue named architecture, hydraulic structures, reinforcement detailing, mechanical parts, and site and urban. The packs built are trusses, isometric drawing, parts, buildings, and sheets, which is how the instructions divided once written.

## For the maintainer

- #362 reads "x": its title and body were replaced on 2026-10-29. Its work is the first bullet above. Restore it if that was a slip.
- Ask the panel for a sheet and open "How this was checked" under the proposal.
- A drawing from a sketch or a scanned plan (#366) is the next milestone, to be planned together before it is filed.
