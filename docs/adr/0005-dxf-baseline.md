# ADR-0005: DXF baseline — hand-written ASCII reader, R12 writer

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves a PROVISIONAL item)  
**Design refs:** §10.1, §24.1, §24.2, §43

## Context

DESIGN §24.1 leaves the initial DXF release target PROVISIONAL and does not say whether to adopt a DXF library. M0 needs to export a line and read it back, and three LOCKED rules constrain how:

- the document stays semantic and must not mirror DXF group codes (§24.1);
- unsupported content is counted and reported, never silently dropped (§24.2);
- imported files are untrusted (§43).

## Decision

1. **`cad-dxf` contains its own reader and writer.** No third-party DXF crate.
2. **The writer emits ASCII DXF release 12 (`AC1009`):** a `HEADER` section with `$ACADVER`, an `ENTITIES` section, `EOF`. Entities are written in `EntityId` order on layer `0`.
3. **The reader is release-agnostic.** It walks group pairs, imports `LINE` records from `ENTITIES`, counts every other entity by type name, counts user block definitions, and skips all other sections. Structural blocks (`*Model_Space`, `*Paper_Space`, `$MODEL_SPACE`, `$PAPER_SPACE`) are not user content.
4. **Coordinates are written as the shortest decimal that parses back to the same `f64`,** always with a decimal point.
5. **Binary DXF is rejected** with a message telling the user to save as ASCII.
6. **The reader is defensive:** iterative (no recursion), bounded by `ImportLimits` (file size, entity count), with a bounded and sanitised unsupported-kind table, and structured errors carrying line numbers.
7. **Properties the drawing cannot hold yet** (layer other than `0`, colour, linetype, thickness, non-zero elevation) are dropped on import and counted, so the UI can warn before a lossy export.

R12 has no mandatory handles, so none are written. When a later release is targeted, handles are generated inside `cad-dxf` and remain unrelated to `EntityId` (§10.1).

## Evidence

- Property test: for arbitrary finite coordinates, export → import reproduces every `f64` bit for bit, and a second export is byte-identical.
- Property tests: arbitrary bytes and arbitrary DXF-shaped text never panic the reader.
- The golden export (`test-data/dxf/valid/cadslate-m0-line.dxf`) is read by ezdxf 1.4.4 as `AC1009` with one `LINE` on layer `0`, exact coordinates, and zero audit errors or fixes.
- Files written by ezdxf 1.4.4 as R12, R2010, and R2018 import correctly, including a mixed-content file whose circles, arc, polyline, text, block, and insert are each reported.
- The R12 fixture exposed a real compatibility detail that the first implementation missed: R12 writers name the model-space container `$MODEL_SPACE`, not `*Model_Space`.

**Verified by hand (2026-10-04):** the maintainer opened the golden export, `test-data/dxf/valid/cadslate-m0-line.dxf`, in a CAD program and it loaded correctly. This check cannot be automated here and was the last open M0 criterion.

## Alternatives considered

- **The `dxf` crate.** Covers far more of the format, but its object model mirrors DXF records, which is what §24.1 tells us to keep out of the document; it would be translated at the boundary anyway. It also brings code for every entity type into the WASM binary and leaves allocation behaviour on hostile input outside our control.
- **Write R2000 (`AC1015`) or later.** Requires handles, subclass markers, a full `TABLES` section, `BLOCK_RECORD`s, and `OBJECTS`. That is a large surface to get exactly right for no M0 benefit: a line is a line in every release.
- **Write only an `ENTITIES` section.** Readable by most tools, but a file that does not state its version invites guessing. The `HEADER` costs four lines.
- **Accept truncated files.** Rejected: a file without `EOF` was cut off, and importing part of a drawing silently is worse than an error.

## Consequences

- The reader and writer together are a few hundred lines with no dependencies, and add little to the WASM binary.
- R12 cannot express `LWPOLYLINE`, true colour, or long symbol names. Polylines (M1) can be written as `POLYLINE`/`VERTEX`; layers and linetypes (M4) fit in R12 tables.
- Every new entity type needs explicit reader and writer support. The compiler enforces the writer side: `Entity` is matched exhaustively.
- Text is decoded lossily as UTF-8. This is harmless while no text content is imported and must be revisited before TEXT is (M4).

## Revisit when

- M5 defines the compatibility matrix and may raise the output release.
- Text import arrives and code pages matter.
- A real CAD tool rejects or misreads a file we write.

## Amendments

### 2026-10-05 — `$INSUNITS` is written into the R12 header

A drawing can now say what unit its numbers are in (DESIGN §14). In DXF that is the header variable `$INSUNITS`, which was introduced after R12. The writer's release stays R12, and the variable is written into its header all the same — only when the drawing has a unit, so a unitless drawing is still a plain R12 file.

Why: DXF is the only way a drawing is saved until M5. Dropping the unit on every save would make the setting useless: reopen the file and `25mm` is refused again.

Evidence:

- The reader takes `$INSUNITS` from the header of any release. Four of the fixtures written by ezdxf (R2010, R2018) declare metres, and now import as metre drawings.
- ezdxf 1.4.4 reads an `AC1009` file with `$INSUNITS` in its header without complaint and reports the unit (`doc.units == 4`); its recovery mode reports no errors for it.
- ezdxf itself does **not** write the variable when asked for R12 ("Drawing units ($INSUNITS) are not exported for DXF R12"). So this is a deliberate step outside what R12 defines, taken because a header variable a reader does not know is one it can skip.

Not verified: that AutoCAD and other tools accept the variable in an R12 header. If one is found that rejects it, the unit moves with the baseline release decision in M5, as the third "revisit when" above already provides.

Codes read and written: 0 unitless, 1 inches, 2 feet, 4 millimetres, 5 centimetres, 6 metres. A file declaring any other unit imports as a unitless drawing, and the import report says so; nothing is rescaled.

### 2026-10-05 — Layers, colours, and linetypes are written; lineweights are reported

M4 gave objects a layer, a colour, a linetype, and a lineweight, and the drawing a layer table and a linetype table (ADR-0013, #138). The writer now writes a `TABLES` section — `LTYPE` with every linetype's pattern, `LAYER` with every layer's colour, linetype, and switches (off as a negative colour, locked as flag 4) — and on every entity its layer (group 8) and, when they are its own, its colour (62) and linetype (6). The header carries the settings for new objects when they are not a new drawing's: `$CLAYER`, `$CECOLOR`, `$CELTYPE`, `$LTSCALE`. All of this is R12.

**Lineweights are not written.** R12 has no group 370; the issue (#139) asked that what the baseline cannot hold be named rather than dropped silently, and that raising the release be M5's decision. So `export_with_report` counts the objects and layers whose lineweight the file loses, and the command log says: *Lineweights are not written in DXF R12: dropped from 2 objects and 1 layer.* The reader takes group 370 from newer files all the same, as it takes `$INSUNITS`, so a lineweight survives a file from another writer — just not our own.

The reader also takes the `LAYER` and `LTYPE` tables of any release; a layer or linetype an entity names but no table defines is made — a new layer's settings, a continuous linetype — and the import report says so. The `ByLayer`/`ByBlock` pseudo-rows newer files carry are not linetypes; the text and shapes of a complex linetype are reported and its dashes kept.

Evidence:

- Two fixtures written by ezdxf 1.4.4 — `ezdxf-r12-layers-linetypes.dxf` and `ezdxf-r2010-properties.dxf` — import as written: switches, patterns, colours, lineweights (R2010), header settings.
- Our export of the R12 fixture is read back by ezdxf with every layer, linetype, pattern, header setting, and entity property intact, and ezdxf's recovery audit reports no errors and no fixes.
- Export → import → export is byte-identical for everything R12 holds (`everything_r12_holds_survives_export_import_export_byte_for_byte`, and a property test over arbitrary tables and property groups).
