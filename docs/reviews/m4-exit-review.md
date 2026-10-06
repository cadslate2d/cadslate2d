# M4 exit review — Drawing Organization

**Date:** 2026-10-06  
**Scope:** DESIGN §32, as rewritten at the M3 exit ([roadmap](../roadmap.md))  
**Outcome:** Every item of the rewritten M4 scope is implemented and tested, an automated scenario exercises them together, and the milestone is closed. M4 was the largest milestone so far — twenty issues, five ADRs, three of them from experiments — and it changed the shape of the document: entities carry properties, tables change through the one history, and a reference's geometry is computed from a table rather than stored. Three things the R12 baseline cannot hold are reported instead of written and are the first work of M5. Several choices about how commands and panels behave are for someone who drafts for a living to judge; they are under *Open items*.

§32 has no exit sentence of its own. The epic's "done when" list stood in for one, and is what the criteria below are checked against.

## The scenario

`web/e2e/m4-scenario.spec.ts` organises a drawing in the production bundle, in Chromium and Firefox:

1. `LINETYPE` loads `Dashed`; `LAYER` makes `walls` with colour 4 and that linetype, and sets it current.
2. A chair — two lines on `walls` — is made a block with `BLOCK`, which replaces the lines with one reference.
3. `INSERT` places it twice more, once at twice the size. While the insertion point is chosen the reference is previewed at the pointer, and **the preview uploads no committed geometry**.
4. Back on layer `0`, `TEXT` writes a label, which `CHPROP` turns yellow.
5. The export is checked table by table: the `LTYPE` row, the `LAYER` row with its colour and linetype, one `BLOCK` holding the two lines on `walls` and none in the model, three `INSERT`s with the scaled one's 41/42/43, the `TEXT` with its colour.
6. The chair is redefined as a tall stool — `BLOCK` asks first — and **every reference changes**, seen in pixels at each of them; the file now holds one line, in the definition, and four references.
7. `LAYER OFF walls` empties the screen of chairs and leaves the label; `ON` brings them back.
8. Every command is undone, one step each, until there is nothing to undo — no lines, references, texts, `walls`, or `BLOCKS` section in the export — and every step is redone: the export is byte for byte the file before.
9. Reload, reopen: "Imported 1 text, 4 block references.", "With 1 layer, 1 linetype, and 1 block.", `LAYER ?` shows `walls` as it was, and the export is the same bytes.

## Done when (epic #103)

| Criterion | Where | Evidence |
| --- | --- | --- |
| Every object carries a layer, colour, linetype, and lineweight, and every table changes through the one transaction history | `cad-document/src/{properties,drawing,transaction}.rs`; [ADR-0013](../adr/0013-entity-properties-and-tables.md) | The history property test in `drawing.rs` mixes entity, layer, linetype, text-style, block, and settings operations and undoes them all; `cad-commands/src/tests/properties.rs` has a test per row of the properties rule in [`docs/modify-matrix.md`](../modify-matrix.md) |
| Layers can be made, set, switched off, and locked from the command line and from a panel; what is on screen follows | `commands/layer.rs`, `web/src/ui/LayersPanel.tsx` | `cad-commands/src/tests/{layer_command,layers}.rs`; `cad-render/tests/render_boundary.rs` (`shown_revision`); `web/e2e/{layer,layers-panel}.spec.ts` |
| Linetypes and lineweights are drawn, stable across segments and zoom | `cad-render/src/scene.rs` (distances along the path), `web/src/renderer/webgl/linetypes.ts`; [ADR-0015](../adr/0015-linetype-rendering.md) | Render-boundary tests of the distance attribute and the pattern table; visual baselines `test-data/screenshots/linetype-*.png` at two zooms; `web/e2e/{linetype,lineweight}.spec.ts` |
| `TEXT` and `MTEXT` exist, with metrics the document can compute without a browser | `crates/cad-text`, `cad-document/src/{text,mtext}.rs`; [ADR-0016](../adr/0016-text-rendering.md) | `cad-text` measures and strokes under plain `cargo test`; bounds and hit-test property tests; `cad-commands/src/tests/{text,mtext}.rs`; `web/e2e/{text,mtext}.spec.ts` with baselines |
| `BLOCK`, `INSERT`, and `EXPLODE` exist; redefining a block is one change that updates every reference | `cad-document/src/{block,exploding}.rs`, `commands/block.rs`; [ADR-0017](../adr/0017-blocks.md) | `redefine_block` is one `Change::Block`; `cad-commands/src/tests/{blocks,block_commands}.rs`; the render-boundary test that a redefinition changes the scene revision once; the property test that an exploded reference occupies where the reference was; `web/e2e/blocks.spec.ts` |
| The selection's properties can be seen and changed | `cad-commands/src/summary.rs`, `commands/chprop.rs`, `web/src/ui/PropertiesPanel.tsx` | `cad-commands/src/tests/chprop.rs`; `web/e2e/properties-panel.spec.ts` |
| All of it survives DXF export and import | `cad-dxf`; [ADR-0005](../adr/0005-dxf-baseline.md), amended four times | Fixtures from ezdxf for tables, properties, text, paragraphs, and blocks; export → import → export byte-identical for every valid fixture; the scenario's reopen. **Except** lineweights and the unity of a paragraph, which R12 cannot hold: reported, and the first issue of M5 (#152) |

Constraints, as the epic set them:

| Constraint | How it was kept |
| --- | --- |
| All persistent table changes use the unified transaction system; no separate undo stacks (§19.2) | One `Change` enum with entity, layer, linetype, text-style, block, and settings variants; one `History`; the property test above |
| Block redefinition is one semantic `BlockChange` that invalidates derived state for dependent references (§19.6) | `Drawing::apply` re-indexes every reference that reaches the changed block and reports them as replaced; the scene is rebuilt once, from `shown_revision` |
| Linetypes stay stable across segments and zoom (§23.2); text rendering is its own subsystem (§23.5). Both start with an experiment and an ADR | ADR-0015 and ADR-0016, each from a probe that still runs (`pnpm --dir web bench:linetype`, `bench:text`) |
| React holds no drawing data; panels act by invoking commands | The panels render `UiState.layers` and `UiState.selection` — names, counts, and strings — and every change they make is a `LAYER` or `CHPROP` script through `CadApp.run`; the lint rules on `web/src/cad/` and `web/src/renderer/` still hold |
| Pointer movement, pan, and zoom upload no committed geometry (ADR-0003) | Asserted by the scenario during `INSERT`'s preview, and by `web/e2e/stretch.spec.ts` as before |
| Dialogs stay narrow and task-specific (§5) | The one dialog added is the paragraph editor: a text area, OK, Cancel |

Test counts at this review: 1242 Rust tests, 119 Vitest tests, 188 Playwright tests in Chromium (171 in Firefox; the visual baselines are compared in Chromium only). At the M3 review they were 998, 94, and 155.

## What was built

| Issue | What |
| --- | --- |
| #131 | Properties beside geometry; one `Change` for entities, tables, and settings; what every modify command does with properties — [ADR-0013](../adr/0013-entity-properties-and-tables.md) |
| #132–#133, #113 | The layer table and the current layer; `LAYER` with every option and the text input kind, which `Space` does not submit; unit suffixes and `UNITS` |
| #134–#136 | A style index per instance and a style table across the boundary — [ADR-0014](../adr/0014-colours-across-the-render-boundary.md); `COLOR` with the index palette, names, `ByLayer`, `ByBlock`; lineweights, `LWEIGHT`, and the `LWT` toggle |
| #137–#138 | Dashes drawn by the fragment shader from a distance along the path — [ADR-0015](../adr/0015-linetype-rendering.md); the linetype table, `LINETYPE`, `LTSCALE`, `LAYER LType` |
| #139 | Layers, colours, and linetypes in DXF both ways; lineweights reported |
| #140–#141 | The Layers panel; the Properties panel and `CHPROP` |
| #142–#145 | A stroke font through the line instances — [ADR-0016](../adr/0016-text-rendering.md) and the `cad-text` crate; `TEXT`, `TEXTEDIT`; `MTEXT` with the host's editor; `TEXT`, `MTEXT`, and `STYLE` in DXF |
| #146–#149 | Blocks as table rows with references resolved through the table — [ADR-0017](../adr/0017-blocks.md); rendering, picking, and snapping through any depth of nesting; `BLOCK`, `INSERT`, `EXPLODE`; the `BLOCKS` section and `INSERT` in DXF |

Beyond the list: `Drawing::query_shown` and `Drawing::shown`, so that what is on a layer that is off is not there for the pointer or the scene; `Command::editor`, the one hook through which a command opens the host's editor; the transaction builder's `touched` set and `others` index, after a bulk `CHPROP` showed its scans were quadratic; `scripts/check-boundaries.sh` knows the new crate.

## Decisions made in M4

| Item | Decision |
| --- | --- |
| Where properties live (§11, PROVISIONAL) | Beside the geometry, in the stored record; `Entity` stays pure geometry; `replace` keeps properties, `insert_like` copies them — [ADR-0013](../adr/0013-entity-properties-and-tables.md) |
| Tables | Rows with stable IDs and a permanent first row (`0`, `Continuous`, `Standard`, `*Model_Space`) that can be restyled but not removed; one `Named` rule for every name, DXF's |
| Colour across the boundary (§23.1) | A style index per instance and a style table of eight bytes a row — colour, kind, weight, pattern; `ByLayer` resolved when the table is; the adapter supplies the theme's colour for a "foreground" row — [ADR-0014](../adr/0014-colours-across-the-render-boundary.md) |
| Lineweights on screen | A byte of the style row; drawn at true size (96 CSS pixels to the inch) and never thinner than the theme's line; shown only while `LWT` is on; not written in R12, reported |
| Linetype rendering (§23.2, EXPERIMENT) | The fragment shader draws dashes from a distance along the path carried per instance; each dash a capsule; a pattern too fine to see is drawn solid; the overlay stays continuous — [ADR-0015](../adr/0015-linetype-rendering.md) |
| Text rendering (§23.5, PROVISIONAL) | Hershey Simplex, 3.5 KB, as line instances through the stroke shader; metrics in `cad-text` on the host; a character the font lacks is a box — [ADR-0016](../adr/0016-text-rendering.md) |
| What a text is | Not a curve: no primitives, picked by its box, snapped only at its insertion; a mirrored text stays readable; `TEXTEDIT` changes the words |
| Paragraphs | A width, a height, wrapping at spaces, `LINE_SPACING` 5/3; edited in the host's editor, opened through `Command::editor`; written to R12 as one TEXT per line |
| Blocks (§19.6) | A definition is a row holding records without IDs; a reference holds an ID and a placement — insertion, one uniform scale, a rotation, a mirror flag — and no geometry; layer `0` and `ByBlock` mean the reference's own; redefinition is one change; cycles refused when defined — [ADR-0017](../adr/0017-blocks.md) |
| `BLOCK` | Name, base point, selection; the selection is replaced by one reference; a taken name asks before redefining |
| `INSERT` | Name, point, scale, rotation, each previewed at the pointer; the pointer's distance and direction stand in for the typed scale and angle |
| `EXPLODE` | One level at a time; parts carry the properties they were drawn with; a polyline becomes lines and arcs; what has no parts is refused by name |
| Blocks in DXF | A mirrored reference is a negative Y scale; a negative X scale is read as the mirror and a half turn; unequal scales, arrays, and attributes are reported and the one reference imported; a cycle is refused |
| Panels | Presentation data only, from `UiState`; every change is a command script |

## What testing found

| Found by | Defect | Fix |
| --- | --- | --- |
| `CHPROP` over 100,000 entities in a benchmark: 30 s | Every `set_properties` scanned the transaction's change list for the entity, so a bulk edit was quadratic | A `touched` set and an index of the non-entity changes; 0.3 s |
| A linetype unit test | `Iterator::sum` of no `f64` is `-0.0`, so an empty pattern had a negative period | `fold(0.0, +)` |
| The first text baseline | Glyph 127 of the Hershey table is a control character and drew a box; brackets rise above the cap height and were clipped by a box the height of the text | `glyph` refuses control characters; `Font::ascent()` measures the tallest glyph |
| The cyclic-blocks fixture, which was *not* refused | The reader took the block names out of its table before resolving the references inside definitions, so every nested reference was "undefined" and dropped — and a cycle through them could not be seen | Names stay in the table while each definition is built |
| The block pick tests | Three failures that were the tests' own: a click on empty space starts a selection box, so the next click finishes it rather than picking; and undo re-selects what it brings back (§19.7) | The tests; the behaviour was right |
| ezdxf's fixtures | Fit text is anchored at its first point; a stacked fraction writes two spaces; R2010 files carry a `Defpoints` layer; a `STYLE` row has a group 42 that an e2e bulge count mistook for a vertex | The reader and the tests, each case recorded in ADR-0005's amendments or the fixture README |
| Vitest after a Rust change | The WASM package was stale: `pnpm wasm` must run before the TypeScript tests | `scripts/check.sh` builds it first |
| The e2e suite under full parallel load, in Firefox | The canvas is intermittently not visible at load | Passes on rerun; not understood, tracked with #109 |

## Measurements

Native x86-64, `opt-level = 3`, except where a browser is named. Chromium's WebGL here is a software rasteriser: its frame times are a rasteriser's, and the finding is what the candidates cost *relative to each other*.

| What | Result |
| --- | --- |
| Colour per instance vs. a style index and table, 100k entities, one frame (ADR-0014) | 3.0 ms vs 0.19 ms building the instance data; the table recolours a layer with no geometry re-uploaded |
| Dashes cut when the scene is built vs. drawn by the shader, 100k entities (ADR-0015) | 11.7 MB vs 2.0 MB of instance data; 37.6 ms vs 1.6 ms to build it |
| Stroke font vs. glyph atlas vs. filled outlines, 10,000 texts (ADR-0016) | 729k segments / 80k quads / 1.46M triangles; the stroke frame six times the atlas frame in the rasteriser; the stroke font ships in 3.5 KB with no renderer change |
| Ten thousand references to one block (`benches/benches/blocks.rs`) | Scene build 4.4 ms; a redefinition with the rebuild 21 ms; a pick 0.3 µs |
| `CHPROP` over 100,000 entities | 30 s before the transaction builder's index, 0.3 s after |

**Not established**, still: frame time on a real GPU (#110). M4 added two instanced attributes (style index, distance) and a texture lookup per fragment; nothing measured here says what they cost on hardware.

## What M4 taught us

- **One change model was the right first move.** Every table since — linetypes, text styles, blocks, settings — was a variant, an arm in `apply`, and operations in one property test. The undo of a layer rename and the undo of a block redefinition cost nothing extra.
- **Properties beside geometry kept every `match` small.** Rendering, picking, snapping, trimming, and DXF read `Entity` as geometry; the record carries the rest. The price is a rule to remember — `insert_like` for anything made *from* an object — and the test per row of the properties table is what makes forgetting it visible.
- **Measure before choosing a transport, again.** Colour per instance looked simpler and would have made recolouring a layer a geometry upload. The table was the cheaper one to build *and* the one that keeps ADR-0003.
- **The shader is the right place for anything view-dependent.** Dashes cut on the CPU would have regenerated committed geometry on every zoom; drawn from a distance along the path they cost one attribute. The same reasoning gave the lineweight to the adapter.
- **A stroke font is a drawing's font.** Nine segments a character sounds like a lot until it is compared with a texture, a second pipeline, and a TrueType parser; and it is one line wide at every zoom, like everything else on the sheet.
- **Geometry computed from a table is a real cost worth paying.** A reference holds no geometry, so a redefinition is one row and every pick, snap, and bound flattens through the table. Ten thousand references pick in 0.3 µs and rebuild in 21 ms; the design principle (§19.6) survived contact with a benchmark.
- **Bulk edits find the scans.** A transaction builder correct for one entity was quadratic for a hundred thousand. Property tests do not find that; a benchmark did.
- **A fixture from another writer is worth three of our own.** ezdxf disagreed with our assumptions six times in M4 — fit text, stacked fractions, `Defpoints`, group 42 in `STYLE`, `$INSUNITS` in R12, attributes — and each time the fixture was right.
- **Tests fail for their own reasons too.** Four M4 test failures were the tests' misunderstanding of settled behaviour — a click on empty space starts a box; undo re-selects. Reading the behaviour before "fixing" it saved two wrong changes.
- **The host's one editor is enough.** `MTEXT` needed a text area and nothing else; `Command::editor` opens it, and the words come back through the command line like anything typed. No dialog framework.

## Open items

Needs a person:

1. **How M4 feels against classic CAD.** `BLOCK` asks for the name before the base point and replaces the selection with a reference; `INSERT` asks scale and rotation every time rather than taking defaults silently; `EXPLODE` takes a nest apart one level at a time; lineweights draw at true size and never thinner than one pixel; the stroke font is Hershey Simplex. Each is defensible and each is a guess about what a drafter expects.
2. **Three things the baseline release loses** — lineweights, the unity of a paragraph, true colour — are reported on export. Whether that is acceptable for long is the release decision, #152, first in M5.
3. Whether AutoCAD and other interactive tools accept `$INSUNITS` in an R12 header (ADR-0005, amended), and read our `BLOCKS` section and `INSERT`s: ezdxf does, with no audit errors; nothing interactive has been tried.
4. Still open from before: the hands-on assessment of the M2 exit condition (#101); the arc grip behaviour against AutoCAD (ADR-0010); render measurements on real GPU hardware (#110); whether to report the R-tree library's bulk-load crash upstream (#111); how the M3 commands feel ([M3 review](m3-exit-review.md)).

Carried forward as issues: the DXF release and what it unlocks (#152, #153); drafting far from the origin end to end (#151); Firefox in CI and any WebKit run (#109).

Known limitations, accepted for now:

- No freeze and thaw, layer transparency, layer states, or filters; no true colour; linetypes are the six built-in patterns, with no `.lin` loading, no complex linetypes, and no linetype scale per object.
- Text is printable ASCII in one stroke font; one text style in effect (`Standard`), the others names; no formatting codes, no fit or aligned justification, no attributes.
- Blocks have one uniform scale and a mirror; no non-uniform scale, arrays, attributes, dynamic blocks, or external references; no `WBLOCK`, `BEDIT`, or block palette.
- The Layers panel and the Properties panel show and change what the commands can; there is no dialog beyond them, and no grip menu or dynamic input near the cursor.
- R12 is written: lineweights are reported, a paragraph is single lines when reopened, and `$INSUNITS` is a step outside the release.

## M5, rewritten

§33 lists candidate work for DXF compatibility and local persistence. With M4 done, the order is ([`docs/roadmap.md`](../roadmap.md) has the reasoning):

1. **The DXF release** (#152): an ADR with evidence, and an R2000 writer, so that lineweights and paragraphs are saved rather than reported. **True colour** decided with it (#153).
2. **The compatibility matrix** (#154) and the **round-trip fixture corpus** from other writers (#155), so that what the reader and writer do is one page and one directory.
3. **Parser hardening** against hostile files (#156), as §43 asks.
4. **The import report and the lossy-export warning in the shell** (#157).
5. **Local recovery persistence**: an ADR on the mechanism, then autosave and recovery (#158); **saving to a file handle** where the browser allows it (#159).
6. **Exit** (#160).
