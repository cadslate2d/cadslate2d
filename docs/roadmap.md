# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | Complete and closed ([review](reviews/m1-exit-review.md)). |
| M2 — OSNAP, Grips, and Basic Modify | Every deliverable is implemented ([review](reviews/m2-exit-review.md)). Open only for the hands-on assessment of its exit condition, which needs a person. |
| M3 — Robust Modify Geometry | Complete and closed ([review](reviews/m3-exit-review.md)). |
| M4 — Drawing Organization | Complete and closed ([review](reviews/m4-exit-review.md)). |
| M5 — DXF Compatibility | Next. Rewritten after M4 and broken into issues. |
| M6–M7 | PROVISIONAL placeholders, to be broken down at the preceding exit review. |
| M8 | FUTURE. Not planned against. |

## M5, as rewritten after M4

DESIGN §33 lists candidate work: broader DXF versions, expanded import and export, a compatibility matrix, unsupported-content warnings, a round-trip fixture corpus, local recovery persistence, and capability-enhanced save handles. §25 constrains the persistence: no Worker introduced for it, and browsers without writable handles still get a polished workflow.

M4 left M5 these pieces: a drawing with layers, linetypes, text styles, blocks, and settings, all of which the R12 writer writes except lineweights and the unity of a paragraph, which it reports; a reader that takes every release and reports what it does not hold; a dozen fixtures from one library; a byte and entity limit on import; a `confirm` dialog before a lossy export; and a download as the only way to save.

### Order of work

1. **The DXF release** (#152). Three things M4 built are lost on every save; ADR-0005's "revisit when" has been reached. An ADR with evidence chooses the release — R2000 is the candidate: lineweights, MTEXT, and everything it requires (handles, `BLOCK_RECORD`, `OBJECTS`) — and the writer gains it. First, because the corpus, the matrix, and the autosave all want to test and store the final form of the file. **True colour** (#153) is decided with the release, since group 420 needs it.
2. **The compatibility matrix** (#154) and the **fixture corpus** (#155). What the reader and writer do becomes one page, every row backed by a test; and fixtures become a directory where a file and a one-line-per-expectation sidecar are a case, as the geometry corpora are, with files from four releases and a place for exports from interactive programs when someone can make them.
3. **Parser hardening** (#156). §43's list — malformed counts, allocation bombs, recursion bombs, oversized entities — becomes limits with tests, hostile fixtures, and a fuzz run in `scripts/stress.sh`. After the release, so the fuzzer knows the writer's own output too.
4. **The import report and the lossy-export warning in the shell** (#157): the §24.2 behaviour as a panel and a narrow dialog that names what will be lost, instead of log lines and a fixed sentence.
5. **Local recovery persistence** (#158): an ADR measuring IndexedDB against the OPFS async API on the main thread, then autosave after each transaction in our own file format and recovery on load. After the release, so that nothing autosaved is lossy. Then **saving to a file handle** where the browser has the API, with the download as the fallback everywhere else (#159).
6. **Exit** (#160): scenario, review, and the M6 breakdown.

### Not in M5

- DWG (§24.3) and opaque passthrough of unsupported records (§24.2: FUTURE).
- Entities the model does not have — dimensions, hatches, splines, ellipses, points — are counted and reported, not imported; dimensions and hatches are M6.
- A native project format (§39) and a Worker for persistence (§25.2).
- Cloud storage of any kind (§43, §45).

### Needs a person

- How M4 feels against classic CAD: `BLOCK`'s order of questions, `INSERT`'s prompts, `EXPLODE` one level at a time, the lineweight display rule, the stroke font ([M4 review](reviews/m4-exit-review.md)).
- Whether interactive CAD programs read our `BLOCKS` section, `INSERT`s, and `$INSUNITS` in an R12 header; ezdxf does.
- How the M3 commands feel against classic CAD: round corners on polyline offsets, JOIN and gaps between arcs, the STRETCH window, BREAK on circles ([M3 review](reviews/m3-exit-review.md)).
- The hands-on assessment of the M2 exit condition.
- Confirming the arc grip behaviour against AutoCAD (ADR-0010).
- Measuring rendering on real GPU hardware.
- Deciding whether to report the R-tree bulk-load crash upstream.

## M4, as it was planned

Kept for the record; the review says how it went.

DESIGN §32 lists candidate work and fixes two things about how: every table changes through the unified transaction system, and redefining a block is one semantic change. The order of work, after M3, was:

1. **Entity properties and the generalized change model**, with an ADR, since everything else stands on it.
2. **Layers**, the **`LAYER` command** with text input for names, and **drawing units**.
3. **Colours across the render boundary**, by measurement, then **object colour** and **lineweights**.
4. **Linetypes**: an experiment and ADR on how dashes are drawn, then the table and commands.
5. **DXF for properties and tables**; the **Layers panel** and the **Properties panel** with `CHPROP`.
6. **Text**: an experiment and ADR on rendering and metrics, then `TEXT`, `MTEXT`, and their DXF.
7. **Blocks**: the model and ADR, then drawing, picking, and snapping, then `BLOCK`, `INSERT`, `EXPLODE`, and DXF.
8. **Exit.**

Left out on purpose: freeze and thaw, layer transparency, states and filters; true colour (decided with M5's release); complex linetypes and a linetype scale per object; text formatting codes, attributes, dynamic blocks, external references, non-uniform block scale; a grip menu; dynamic input near the cursor.

## M3, as it was planned

Kept for the record; the review says how it went.

DESIGN §31 lists candidate work and says the exact scope should be rewritten once real M2 geometry exists. Its order of work was:

1. **The polyline bulge model**, then DXF bulges and arc segments in `PLINE`.
2. **Perpendicular and tangent snaps.**
3. **Hover feedback for object picks**, then **TRIM and EXTEND** with several edges at once and with polylines as targets.
4. **OFFSET**: arcs, then polylines after an experiment and an ADR.
5. **FILLET and CHAMFER** with arcs and circles, then on polylines.
6. **BREAK, JOIN, STRETCH.**
7. **Exit.** The regression corpus and stress tests grew alongside, with one issue for the harness.

## Standing rules

- A PROVISIONAL or EXPERIMENT item is resolved by an ADR with evidence, not by default.
- Every milestone ends with a review like M0's: criteria linked to tests, lessons recorded, the next milestone's issues adjusted.
- Performance targets are checked with the generated drawings in `benches/` at 10k, 50k, and 100k entities.
