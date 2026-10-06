# ADR-0018: The DXF release is R2000

**Status:** Accepted (2026-10-06)  
**Date:** 2026-10-06  
**Kind:** Evidence (resolves DESIGN §24.1, PROVISIONAL; supersedes the release decision of [ADR-0005](0005-dxf-baseline.md))  
**Design refs:** §24.1, §24.2, §33, §41, §46

## Context

ADR-0005 chose ASCII DXF release 12 as the M0 baseline because a line is a line in every release, and listed as its first "revisit when": *a user needs an entity or property R12 cannot carry*. M4 built three:

- **Lineweights** (#136). R12 has no group 370. The writer counted them and the command log said *Lineweights are not written in DXF R12: dropped from 2 objects and 1 layer.* on every save of a drawing that used them.
- **Paragraphs** (#144). R12 has no MTEXT. A paragraph was written as one TEXT per line; reopened, it was single lines with no width and no unity.
- **True colour**, deferred to M5 because group 420 needs a release newer than R12 (#153).

So a drawing saved and reopened was not the drawing. §24.2 (LOCKED) forbids pretending otherwise, and the writer did not pretend — but the honest report is not a fix. The issue (#152) asks for the release that holds what the drawing does, chosen with evidence.

## The candidates

**R12 (`AC1009`)**, kept, with the losses reported. Simplest; what every tool reads; and the one release in which the file is only the drawing, with no handles, dictionaries, or layouts.

**R2000 (`AC1015`)**. The first release with lineweights (370 on entities, layers, and `$CELWEIGHT`), MTEXT, and LWPOLYLINE (true colour, group 420, came with R2004); the release most tools default to when asked for "an old DXF"; still read by everything. It requires: a handle (group 5) on every record and an owner (330); subclass markers (100); a `CLASSES` section; nine tables including `BLOCK_RECORD`; `*Model_Space` and `*Paper_Space` blocks; and an `OBJECTS` section with the named-object dictionary and the layouts that tie the spaces together.

**R2010 (`AC1024`) or later**. Nothing the drawing holds needs them; they add more objects to write and nothing to gain until M6 (dimension styles are in R2000 too).

## Evidence

**What a reader requires.** The writer was built from a file ezdxf 1.4.4 writes for R2000, pruned section by section with ezdxf's recovery audit as the judge (`recover.readfile`, which reports errors and the fixes it had to make). The full file: 3,134 lines for seven entities. Without the 300-variable header, the `CLASSES` content, and the `VIEW`, `UCS`, `APPID`, `DIMSTYLE` tables: no errors. Without the `OBJECTS` section entirely: no errors and no fixes. ezdxf is tolerant; AutoCAD is not, and the DXF reference does not say what it needs. So the writer writes what the reference and ezdxf agree a complete file has, and no more: the nine tables (`VIEW` and `UCS` empty, `APPID` with `ACAD`, `DIMSTYLE` empty, one `VPORT`), an empty `CLASSES`, the two spaces, and an `OBJECTS` section with the root dictionary, `ACAD_GROUP`, `ACAD_LAYOUT` with a layout per space pointing at its block record and pointed at by it, and `ACAD_PLOTSTYLENAME` with the one placeholder every layer names in group 390.

**What ezdxf makes of the output.** Our export of three fixtures — nested blocks placed every way; layers, lineweights, and a complex linetype; five paragraphs with formatting, attachment points, and 300 characters in pieces — reads back in ezdxf as `AC1015` with **0 audit errors and 0 fixes**, every layer's lineweight (`-3` for the default, `100` for the heavy one), every entity's lineweight, both layouts, every block, and each MTEXT with its height, width, attachment point, and `\P` break.

**What survives.** `everything_the_drawing_holds_survives_export_import_export_byte_for_byte`: a drawing with two layers, two linetypes, lineweights on a layer and on entities, every settings variable, and lines with every kind of property exports, imports, and exports again to the same bytes, and the lineweights come back as they went. `a_paragraph_is_written_as_mtext_with_breaks_and_codes_escaped_and_survives`: a paragraph with a break and with `\`, `{`, `}` in its words, and one of 300 characters, round-trips with no formatting dropped. Every fixture under `test-data/dxf/valid/` — sixteen files, from ezdxf across R12, R2010, and R2018 and our own — passes the same round trip, and the paragraphs are now paragraphs on the far side.

**What it costs.** The nested-blocks fixture re-exported: 1,338 bytes as R12, 4,676 as R2000. A line is eleven groups instead of five. The reader does not care: it already skipped handles, markers, and the `OBJECTS` section of every modern fixture.

**Not established.** That AutoCAD, BricsCAD, LibreCAD, or QCAD open the output. None runs here; the issue says so and the review carries it as *needs a person*. The output follows ezdxf's R2000 record for record, and ezdxf's files open in all of them.

## Decision

### 1. The writer writes R2000, and only R2000

`$ACADVER` is `AC1015`. There is no R12 option: one writer, one golden file, one set of fixtures. A drawing that needs an R12 file can be converted by any CAD tool; the reader still takes R12 files, as it takes every release.

### 2. The file is complete, and no larger than that

`HEADER` with `$ACADVER`, `$DWGCODEPAGE`, `$INSUNITS` when the drawing has a unit, the settings for new objects when they are not a new drawing's, `$EXTMIN`/`$EXTMAX` when the drawing has anything in it, and `$HANDSEED`. An empty `CLASSES`. `TABLES`: `VPORT` with one `*Active` viewport looking at the drawing's extents; `LTYPE` with `ByBlock`, `ByLayer`, and the drawing's rows, each pattern element followed by its type; `LAYER` with colour (negative when off), linetype, lineweight, and the shared plot style; `STYLE`; `VIEW` and `UCS`, empty; `APPID` with `ACAD`; `DIMSTYLE`, empty; `BLOCK_RECORD` with the two spaces, pointing at their layouts, and one row per block. `BLOCKS` with the two spaces, empty, and every definition, its entities owned by its record. `ENTITIES`, owned by model space. `OBJECTS` as the evidence describes.

### 3. Handles follow entity IDs

Nine structural handles are allocated first, then the block records, then every table, row, block, and entity in writing order. Entities are written in `EntityId` order, as before, so the same drawing is the same file, byte for byte, and the round-trip tests stay meaningful. The reader ignores handles, so a file's handles never reach the drawing.

### 4. What R2000 carries that R12 did not

Lineweights on entities (370), on layers (370, `-3` for the default), and in the header (`$CELWEIGHT`). Paragraphs as MTEXT: top-left insertion, height, width, attachment point 1, the style when not `Standard`, the words in pieces of 250 characters with `\P` for a break and `\`, `{`, `}` escaped, and the rotation. Polylines as LWPOLYLINE — one record instead of a header, a vertex record each, and a `SEQEND` — with the bulge after its vertex. The `ExportReport` that counted what R12 lost is gone: nothing the drawing holds is lost.

## Alternatives considered

- **Stay at R12 and report.** The honest baseline, kept for four milestones. Rejected because the report had become the file's most-read line: every save of a drawing with a lineweight or a paragraph lost something.
- **Offer both releases.** A `DXFOUT` option for R12. Rejected for now: two writers to keep byte-identical, two golden files, every fixture round-tripped twice, and no one who has asked. The "revisit when" below keeps the door.
- **R2010 or R2018.** Nothing to gain for the drawing M5 has; more objects to write exactly right.
- **Write only what ezdxf needs.** The pruning experiment showed ezdxf accepts a file with no `OBJECTS` at all. Rejected: the one reader we can run is the lenient one, and the cost of the complete structure is a few hundred bytes.

## Consequences

- `cad-dxf`'s writer is `Writer` with a handle allocator; `export_with_report` and `ExportReport` are removed from the API, and the WASM `export_dxf` no longer writes report lines to the log.
- The golden file `test-data/dxf/valid/cadslate-m0-line.dxf` is regenerated; `every_valid_fixture_round_trips_stably` holds entity counts equal, paragraphs included.
- The end-to-end suite reads exports through a record parser in `web/e2e/support.ts` (`entities`, `lines`, `circles`, `polylines`, `tableRow`, `blocks`, …) instead of matching R12 substrings; every spec that checked a file was updated.
- ADR-0005's amendments on units, tables, text, and blocks stand; its release decision is superseded here.
- DESIGN §24.1's PROVISIONAL release is resolved.

## Amendments

### 2026-10-06 — True colour is not adopted; a file's true colour is read as the nearest index

The roadmap deferred true colour to this decision because group 420 needs a release newer than R12 — R2004, in fact, one more than the R2000 chosen above (#153). With the release here, the question is the colour model's, not the file's, and the answer is no — for now.

Why: the drawing's colour model is the index palette — 255 numbered colours, `ByLayer`, `ByBlock` — because that is what command-driven drafting uses: a colour is a number you type, a layer's colour is a number in a list, and the palette is the same in every drawing. A true colour would be a second kind of colour in every place the first appears: `COLOR`, `LAYER Color`, `CHPROP`, the two panels, the style table (which carries RGB already), the DXF reader and writer, and the ACI names. None of M5's work needs it; nothing a drafter has asked for needs it; and §45 asks that the product stay small. It stays out until a drawing that needs it shows up.

What a file with true colours becomes: the reader takes group 420 on an entity or a layer and gives the object **the nearest index colour** — least distance in RGB, the lower number on a tie, computed by `cad_document::nearest_index` from the palette's own table — and reports it: an entity counts among those whose properties the drawing cannot hold, a layer as `(layer true colour)`. The group 62 that writers put beside a true colour is not trusted: AutoCAD writes the nearest index there, ezdxf a default (`7`), so a teal layer would come in white. The object is never lost, and the export warning applies, as §24.2 asks.

Evidence: `ezdxf-r2010-true-colour.dxf` — a teal layer, an orange line (→ 30), a near-blue line with a `62` of 5 beside its colour (→ 5), a pure-red circle (→ 1) — imports as described and says so; every palette colour is its own nearest index (`every_palette_colour_is_its_own_nearest_index`).

Revisit when a drawing needs a colour the palette lacks: the style table is already RGB, so the renderer is ready; the work is in the commands, the panels, the names, and the file.

## Revisit when

- A user needs an R12 file from CadSlate2D itself and no tool between can convert it: add the option, as a second writer behind the same `export`.
- An interactive CAD program refuses the output: the first such report is a fixture and a fix.
- M6 needs something R2000 lacks (it should not: `DIMENSION`, `HATCH`, and `DIMSTYLE` are all R2000).
