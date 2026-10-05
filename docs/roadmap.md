# Roadmap

The plan of record is the set of GitHub milestones and issues, in number order. This page explains the shape of the plan and what changed after each milestone review. DESIGN.md Parts III–IV remain the source for scope.

## Where we are

| Milestone | State |
| --- | --- |
| M0 — The Line | Complete and closed ([review](reviews/m0-exit-review.md)). |
| M1 — Input and Basic Entities | Complete and closed ([review](reviews/m1-exit-review.md)). |
| M2 — OSNAP, Grips, and Basic Modify | Every deliverable is implemented ([review](reviews/m2-exit-review.md)). Open only for the hands-on assessment of its exit condition, which needs a person. |
| M3 — Robust Modify Geometry | Complete and closed ([review](reviews/m3-exit-review.md)). |
| M4 — Drawing Organization | Next. Rewritten after M3 and broken into issues. |
| M5–M7 | PROVISIONAL placeholders, to be broken down at the preceding exit review. |
| M8 | FUTURE. Not planned against. |

## M4, as rewritten after M3

DESIGN §32 lists candidate work: layers, colours, linetypes, lineweights, blocks, insert, explode, text, multiline text, and a properties UI. It fixes two things about how: every table changes through the unified transaction system, and redefining a block is one semantic change.

M3 left M4 these pieces: entities that are geometry and nothing else, on an implied layer 0; a `Change` with one variant; a render boundary that carries style roles but no colours; stroke width already applied in the shader; a DXF reader and writer that know one layer; a command line that takes points, numbers, keywords, and picks, but no free text; and a shell with a toolbar and no panels.

### Order of work

1. **Entity properties and the generalized change model.** Entities gain a layer, a colour, a linetype, and a lineweight, and `Change` gains its table variants. This comes first because layers, linetypes, text styles, and blocks are all tables, and every later issue assumes objects carry properties. It starts with an ADR, since where properties live decides how much every `match` on an entity has to know about them. It also settles what each modify command does with properties: which object a fillet arc or a joined polyline takes them from.
2. **Layers**, then the **`LAYER` command**. The command line is the primary interface, so layers are complete there before there is a panel. Layer names are the first free text the command line takes, which brings a text input kind that `TEXT` reuses. **Drawing units** (already written up as an issue) follow, since they are the second drawing setting.
3. **Colours across the render boundary.** How resolved colours reach the GPU — a style table looked up per instance, or a colour per instance — is a transport decision like ADR-0008's, made the same way: by measuring. Then **object colour** and **lineweights**, which travel the same path.
4. **Linetypes.** §23.2 wants dashes stable across segments and zoom. Whether they are cut when the scene is built or drawn by the shader from a distance along the path is an experiment and an ADR, as curve rendering was. The table and commands follow.
5. **DXF for properties and tables**, so that nothing M4 adds is lost on save. Then the **Layers panel** and the **Properties panel**, which act only by invoking commands and show only presentation data.
6. **Text.** Its rendering is PROVISIONAL (§23.5), and two constraints narrow it: the document needs text extents without a browser, and M6's dimensions wait on reliable metrics (§34). So it starts with an experiment and an ADR, then `TEXT`, `MTEXT`, and their DXF.
7. **Blocks.** The model first, with an ADR, because §19.6 fixes how redefinition must behave before any code exists: one change, however many references. Then drawing, picking, and snapping references; `BLOCK`, `INSERT`, and `EXPLODE`; and DXF.
8. **Exit:** scenario, review, and the M5 breakdown.

### Not in M4

- Freeze and thaw, layer transparency, layer states and filters. The visual reference shows some of them; §32 does not list them.
- True colour, which needs a DXF release newer than the baseline: decided with M5.
- Complex linetypes; a linetype scale per object.
- Text formatting codes; attributes, dynamic blocks, external references, non-uniform block scale.
- A grip menu (Lengthen, Radius, add or remove vertex). It was a candidate for M4 "alongside properties"; M4 is already the largest milestone so far, and nothing in it depends on the menu.
- Dynamic input near the cursor (§13.5).

### Needs a person

- How the M3 commands feel against classic CAD: round corners on polyline offsets, JOIN and gaps between arcs, the STRETCH window, BREAK on circles ([M3 review](reviews/m3-exit-review.md)).
- The hands-on assessment of the M2 exit condition.
- Confirming the arc grip behaviour against AutoCAD (ADR-0010).
- Measuring rendering on real GPU hardware.
- Deciding whether to report the R-tree bulk-load crash upstream.

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
