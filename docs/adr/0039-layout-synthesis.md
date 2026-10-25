# ADR-0039: Floor plans are laid out by a deterministic search over slicing trees, in a pure crate of their own, `cad-layout`

**Status:** Proposed  
**Date:** 2026-10-25  
**Kind:** Evidence (a new subsystem and crate; amends ADR-0001's dependency graph)  
**Design refs:** §3, §4, §21, §43–§46; ADR-0001, ADR-0020, ADR-0037, ADR-0038

## Context

M22 (#276) asks for the first domain generator. Its input is a terrain, rooms with target areas and adjacencies, and 25 cm exterior and 15 cm interior walls. Its output is clean, editable walls, doors, labels, and areas. It should work as a `FLOORPLAN` command without AI. Through the AI, the brief comes from plain language and revisions ("make the kitchen bigger") become changes to the brief.

The problem is **layout synthesis**, not drawing. Rooms must be placed so that:
- none overlap;
- all lie inside the buildable area;
- each keeps its area, its minimum width, and its proportions;
- the rooms asked to open onto each other share a wall long enough for a door;
- every room can be reached from the entrance.

The drawing that follows from a layout — wall faces, junctions, doors, labels — is #279 and #280. This ADR decides the brief, the solving method, and where the code lives (#277). The issue asks for a prototype's success rate and time on a corpus of 30 briefs; the prototype is the crate this ADR proposes.

DESIGN §4 rules out BIM: walls stay plain geometry on layers, never wall objects.

## The prototype

`crates/cad-layout`: the brief (`Spec`), the search (`solve`, `alternatives`), and an independent check of a plan against its brief (`verify`). It is about 1,400 lines with its tests and the corpus runner. The corpus is `test-data/layout/briefs.jsonl`: 30 briefs, from a studio to a 20-room office floor, among them:
- narrow, shallow, and set-back lots;
- strict minimum widths and aspect ratios;
- a hall that must open onto five rooms, and a chain of five rooms in a row;
- Portuguese room names;
- six briefs that cannot be built:
  - too much floor for the land;
  - setbacks that leave nothing;
  - a 2 m² bath that must be 1.6 m wide both ways;
  - a 2.5 m² wc that must open onto five rooms;
  - a 6 m hall on a 6 m lot;
  - twelve rooms whose floor fits the land but not with the walls between them.

`cargo run --release -p cad-layout --example briefs` runs them. Times are native, release, on the development machine (2026-10-25):

| | Result |
| --- | --- |
| Briefs as expected | **30/30**: the 24 feasible ones planned, every plan passing `verify`; the 6 infeasible ones refused, each with the reason the brief expects |
| Time per brief, one plan (up to 4 seeds) | p50 **19 ms**, p90 175 ms, max 424 ms (the 20-room office floor and the twelve-room overflow) |
| Time per brief, three alternatives (up to 9 seeds) | p50 54 ms, p90 509 ms, max 1.4 s |
| One search alone, seeds 0–9, feasible briefs | 199/240 (83%) find a plan; the hall that opens onto five rooms is the hardest, 1/10 |
| Determinism | the same brief and seed make the same plan, bit for bit (a test, and the property test) |

![A twelve-room villa as the prototype laid it out](images/0039-villa.svg)

*Each room's floor (white) inside its walls (dark), its centreline rectangle dashed, and a dot on each wall that takes a door. The building sits at the front setback, centred.*

The first run found 22 of 30. Every miss was the same structural limit: a slicing fills its rectangle, so a 25 m² living room beside a 4 m² bath cannot keep both rooms' proportions. Real plans solve this with circulation, and so does the prototype: the search may add one **circulation space**, penalised by its floor, so it appears only when it buys a feasible plan. With it, 30 of 30. Longer searches and hotter starts were measured too, and changed little (202/240 single searches at twice the steps); restarting from other seeds is what makes the method reliable.

## Decision

### 1. The brief

One JSON shape (`cad_layout::Spec`), in metres, for a spec file, for `FLOORPLAN`'s prompts, and for the intent IR:

| Field | Meaning | Default |
| --- | --- | --- |
| `terrain` | `width` (along the front, x) and `depth` (y) of a rectangular plot; the front lies along the x axis from the origin | — |
| `setbacks` | `front`, `back`, `left`, `right` | 0 |
| `rooms` | `name`, `area` (the floor inside the walls), `min_width`, `max_aspect`, `exterior` (wants an outside wall), `entrance` (the way in) | `min_width` 1.5 m, `max_aspect` 2.5, the first room the entrance |
| `adjacent` | pairs of room names that must share a wall with a door | none |
| `exterior_wall`, `interior_wall` | thicknesses | 0.25 m, 0.15 m |
| `door` | clear width of a door | 0.9 m |
| `seed` | the search's seed | 0 |

The brief is checked before any search (`Spec::check`): sizes positive, names unique and known, at most 24 rooms.

### 2. The geometry

- **The building is one rectangle,** its outline the exterior walls' centreline. It sits on the front setback, centred along it, and its width is a variable of the search.
- **The rooms tile the outline** as a slicing: cut it across or along, then cut each part again, until each part is a room.
- **Areas are measured inside the walls.** A room's floor is its centreline rectangle less half the thickness of the wall on each side: exterior or interior, by whether that side lies on the outline. A room's area is its floor's.
- **Sizes follow from the slicing.** Each cut divides its rectangle in proportion to the gross areas on either side. The gross areas are corrected for the walls six times, a fixed point, so each floor meets its target. Within the hard ±5% they are usually exact.
- **A door needs** its width, 0.15 m clear of each end, and the thickness of the wall crossing at each end: 1.45 m of shared centreline at the defaults.

### 3. The constraints

**Hard**, each checked with what it costs to break, and again by `verify`:
1. inside the buildable area;
2. every room's floor within **±5%** of its area;
3. every room's narrower side at least its minimum width;
4. its aspect at most its maximum;
5. each `adjacent` pair sharing a wall long enough for a door;
6. the entrance having an outside wall long enough for one;
7. every room reached from the entrance, through doors on walls long enough for one.

The rooms do not overlap and leave no gap by construction: a slicing tiles its rectangle. `verify` checks those too.

**Soft**, as the discussion put them:

> E = w_a·E_area + w_r·E_ratio + w_c·E_connectivity + w_p·E_preferences, plus the circulation's cost

| Term | What it measures | Weight |
| --- | --- | --- |
| E_area | Σ relative area error | 10 |
| E_ratio | Σ ln(aspect): squarer is better | 1 |
| E_connectivity | mean number of doors from the entrance | 0.5 |
| E_preferences | rooms that want an outside wall and have none long enough for a window | 2 |
| circulation | its floor over the rooms' floor | 5 |

A hard constraint's breach weighs 100 per room or per metre, so a plan that keeps them always beats one that does not.

**The doors** are the `adjacent` pairs, then a breadth-first spanning tree from the entrance over the walls long enough for a door; the asked-for walls come first. They become openings in #280.

### 4. The search

- **A slicing is a normalised Polish expression** (Wong and Liu, 1986): room operands and V/H cuts in postfix, never the same cut twice in a row. Every slicing has exactly one.
- **Simulated annealing** moves through them:
  - two neighbouring rooms swapped (M1), or any two;
  - a chain of cuts complemented (M2);
  - a room and a cut swapped where the result stays normalised (M3);
  - the outline made wider or narrower;
  - the circulation space made, grown, shrunk, or taken away.
- **3,000 steps per room,** the temperature falling geometrically from 10 to 0.005. Steps whose move would break the form are skipped.
- **Deterministic:**
  - a seeded SplitMix64 generator;
  - no hash maps, no threads, no clock;
  - the same brief and seed, the same plan on every machine.
- **`solve`** is one search from the brief's seed. **`alternatives(n)`** searches from that seed and the ones after it, up to max(3n, 4) of them. It keeps the plans that keep every hard constraint, distinct to a centimetre, and ranks them by their soft energy.

**The circulation space** is an extra leaf of the slicing whose floor is a variable: none, or between 1.5 m² and 30% of the rooms' floor. When it is there, it is called `circulation`, is at least a door wide, and takes doors like a room. It is not in the brief, so it has no area target, and the drawing labels it `circulation`.

### 5. Infeasibility, with reasons

- **Before any search**, what can be said exactly:
  - the setbacks leave nothing to build on;
  - the rooms' floor exceeds what the buildable area holds inside its exterior walls;
  - a room's area is smaller than its minimum width squared.

  These come back in microseconds.
- **After the search**, the best plan any seed found is explained, a sentence per broken constraint:
  - "The hall could not be made 6 m wide."
  - "The wc and the b could not share a wall long enough for a door."

  The message names the constraint the search could not keep. It does not prove that no plan exists.

### 6. Where it lives

A new crate, **`cad-layout`**, depending on `cad-core` only (for the tolerance classes). It is pure, deterministic, and host-testable, like `cad-geometry`.

`cad-commands` will depend on it for `FLOORPLAN` (#281). The walls, labels, and doors (#279, #280) turn a `Layout`'s rectangles into ordinary entities on `WALL-EXT` and `WALL-INT` — lines and polylines, never wall objects (§4).

The crate is optimised in debug builds too (`[profile.dev.package.cad-layout]`): its tests would otherwise be ten times slower.

ADR-0001's graph gains `cad-layout → cad-core` and `cad-commands → cad-layout`.

## Alternatives considered

| Method | Why not |
| --- | --- |
| **Mixed-integer programming** (big-M non-overlap) | It would be exact, and could prove infeasibility. But it needs a solver dependency: the open ones are C/C++ (CBC, HiGHS) or heavy for WASM. Its run time on 20 rooms varies by orders of magnitude, and its result can change with the solver's version. |
| **Constraint programming** on a grid | Areas within 5% need a fine grid, and the search space grows with the cells. It too needs a solver we do not have. |
| **Rectangular duals** of the adjacency graph | Exact for adjacency, but only for a planar triangulated graph. Briefs rarely give one, and areas and widths come after, by another optimisation. |
| **Exhaustive slicings** | The number of slicings grows like the Catalan numbers: millions at 12 rooms. |
| **Genetic algorithms** | The same representation with more parameters to tune. Annealing is simpler to make deterministic. |
| **A model drawing the plan** | It is not deterministic, not testable, and not explainable when it fails. ADR-0037 keeps the model to filling the brief, and the solver decides. |

## Consequences

- **Plans are slicings of one rectangle.** Pinwheel arrangements, L- and U-shaped buildings, courtyards, and non-rectangular rooms cannot be made. Terrains are rectangles. That is the MVP the epic set out. An L-shaped plot or building is a later extension: several rectangles, or a slicing with blocked leaves.
- **One storey, one rectangle of land.** Stairs, levels, and orientation (sun, street side) are not modelled. "Front" is only the setback's side and where the building sits.
- **A circulation space may appear unasked.** It is labelled as such, and the brief can name a hall of its own to have one where it wants.
- **Every request may take up to half a second** at 20 rooms, and alternatives three times that. That is fine for a command and for the AI. Live dragging of a room is not offered; edits change the brief and the plan is made again.
- **A refusal names a constraint, not a proof.** A brief at the edge of feasibility may be refused by four seeds and planned by a fifth. `alternatives` with more seeds is the remedy, at its cost.

## Revisit when

- Briefs need non-rectangular buildings or plots, more than one storey, or more than 24 rooms.
- A brief the corpus says is feasible takes more than a second, or is refused at the default seeds.
- Users edit plans so much after generating them that a layout which keeps its constraints under edit (ADR-0038) would serve better than making the plan again.
