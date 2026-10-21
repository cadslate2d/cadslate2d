# M19 exit review — DWG

**Date:** 2026-10-21  
**Scope:** reading and writing DWG in the AutoCAD 2000, 2004, and 2007/2008 formats (`AC1015`, `AC1018`, `AC1021`), asked for by the user on 2026-10-18 ([roadmap](../roadmap.md), epic #250, [ADR-0036](../adr/0036-dwg.md))  
**Outcome:** Every item is implemented, measured, and tested; what remains needs a person.
- A crate of our own, `cad-dwg`, written from the Open Design Specification, reads the three containers and every object the drawing holds, into the same `Drawing` the DXF reader makes, through the same records, report, and limits.
- It writes the three containers — the R2000 sections, the R2004 pages and an LZ77 of ours, the R2007 Reed–Solomon pages, CRC-64s, and string streams — and the output is read back by three readers: ours byte for byte, acadrust with nothing lost, LibreDWG without an error.
- `OPEN` tells a DWG by its first six bytes; `SAVEAS` asks the format and `SAVE` keeps the one the file came in.
- An automated scenario opens a DWG of each version, drafts on it, saves it as each of the four formats, and reopens every save.

The milestone is closed. The check in AutoCAD needs a person: #321, under M15.

## The scenario

`web/e2e/m19-scenario.spec.ts`, in Chromium and Firefox. For each of `AC1015`, `AC1018`, and `AC1021`:

1. **OPEN** the mixed-content fixture of that version — three lines, two circles, an arc, a polyline, a text, a block and its reference, on two layers — and the log counts them; the status bar names the file and does not say LOSSY.
2. **Draft on it:** a line and a circle.
3. **SAVEAS** as each of `DXF`, `DWG2000`, `DWG2004`, and `DWG2007`. Each file carries its signature and the format's extension; the log says what it was exported as; the unsaved marker goes.
4. **OPEN** each save: four lines and three circles now, the rest as before, the two layers and the block; not LOSSY; and the next `SAVEAS` offers the format the file came in.

Twelve saves, twelve reopens, no line of the log refused.

## Done when (epic #250)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| The ADR, with the spike's measurements | ADR-0036 | The spike's table (2026-10-20); the exit's measurements below |
| Fixtures in all three versions from an independent writer, each importing to the same drawing as its DXF source | #312, #313, #314 | 99 fixtures under `test-data/dwg/valid/` — 32 DXF sources in three versions, written by acadrust 0.6.5 through `examples/fixtures.rs`, and three crafted for what no writer makes — and 6 malformed; `every_fixture_makes_the_drawing_its_dxf_source_makes()` compares entities, properties, and table rows, with acadrust's own losses listed (`WRITER_LOSSES`) and checked against LibreDWG's reading |
| Our DWG read back by an independent reader with nothing lost, in all three versions; export → import → export byte-identical | #315, #316, #317 | `acadrust_reads_our_output_with_nothing_lost()`, `every_fixture_round_trips_through_our_dwg_byte_for_byte()` over 41 DXF fixtures × 3 versions; `scripts/dwg-check.sh`: LibreDWG's `dwgread` read all 123 outputs without an error |
| Arbitrary bytes never panic: a fuzz target and malformed fixtures | #312 | `fuzz/fuzz_targets/dwg_read.rs`; four ten-minute runs seeded with the fixtures and our output (882,559, 1,118,506, 947,492, 870,038 runs), nothing found; six malformed fixtures refused with a structured `DwgError`; every bound of `ImportLimits` reached by a test |
| A DWG matrix naming each record's test or fixture | #312–#317 | [`docs/dwg-matrix.md`](../dwg-matrix.md): 104 rows, every name checked to exist by `the_matrix_names_only_tests_and_fixtures_that_exist()` |
| An e2e flow: open a DWG 2000, 2004, and 2007, edit, save as each version, reopen | #319, #320 | `files.spec.ts` (`a DWG edited and saved as each version reopens with the edit`), `m19-scenario.spec.ts` |
| Opened in AutoCAD or DWG TrueView without an audit error | needs a person | Carried to #321 (M15) |

## The registry sweep

What the DXF matrix holds against what the DWG matrix holds, record by record:

- **Every entity DXF import holds, DWG import holds:** `LINE`, `CIRCLE`, `ARC`, `POINT`, `ELLIPSE`, `XLINE`, `RAY`, `LWPOLYLINE`, `POLYLINE` with `VERTEX` and `SEQEND`, `TEXT`, `MTEXT`, `ATTRIB`, `ATTDEF`, `INSERT` (and `MINSERT`, reported as an array), `SPLINE`, `HATCH`, `WIPEOUT`, `DIMENSION` of every kind, `LEADER`, `MULTILEADER`, `VIEWPORT`.
- **Every table and object:** `LAYER`, `LTYPE`, `STYLE`, `DIMSTYLE`, `MLEADERSTYLE`, `BLOCK_RECORD`, `APPID`, `VPORT`, `UCS`, `VIEW`, `LAYOUT`, `GROUP`, `SORTENTSTABLE`, `XRECORD` (the layer states), `DICTIONARY`, `WIPEOUTVARIABLES`.
- **Every header variable the DXF matrix lists** (`$CLAYER`, `$CELTYPE`, `$CECOLOR`, `$CELWEIGHT`, `$TEXTSTYLE`, `$DIMSTYLE`, `$CMLEADERSTYLE`, `$INSUNITS`, `$LTSCALE`, `$PDMODE`, `$PDSIZE`, `$EXTMIN`/`$EXTMAX`, `$HANDSEED`) is read from the header section and written back; `$ACADVER` and `$DWGCODEPAGE` are the file's own.
- **What DXF import reports, DWG import reports the same way:** proxy entities, `MLINE`, `TOLERANCE`, regions, solids, bodies, shapes, OLE frames, meshes, images, tables, and every class entity the crate does not decode, each counted under its name; gradient hatches, arrays, non-uniform scales, nested blocks past the limit.
- **What DXF import holds that DWG does not:** nothing. The one asymmetry is in the writer: a spline given by control data of its own and fit points too keeps the control data (a DWG spline is one or the other, §20.4.40); the fit points are named before the save by `export_losses`. A drawn spline, whose control data is what a spline through its fit points has, round-trips whole.
- **The specification errata** table has 16 rows, each confirmed in LibreDWG's source and the files: the R2004 "nolinks" bit, ENC's long, the object map section sizes, the literal copy's byte order, the DICTIONARY type, CMC's method byte, the layer's "on" bit, the LAYOUT viewport count, the XRECORD string length, the layout's point order, the header's trailing shorts, the per-section object map offsets, `MENUNAME` in the R2007 string stream, and the rest.

## Measurements

Against ADR-0036 §5, on this machine (Linux, WSL2), release builds on the host.

**The WASM.** Built as the ADR measured the spike: a `cdylib` of the reader and writer with the DXF crate, `opt-level = "s"`, LTO, `wasm-opt -Os`, gzipped at level 9.

| | Raw | Gzipped |
| --- | --- | --- |
| DXF reader and writer alone | 595,123 | 244,655 |
| With `cad-dwg`, reading and writing all three versions | 894,775 | 350,650 |
| **Added** | **299,652 (293 KB)** | **105,995 (104 KB)** |
| Budget | 500 KB | 200 KB |

Within the budget, with the full writer: 266 KB and 92 KB when #315 measured the AC1015 writer alone.

**The benchmark drawings** (`benches/generated-drawings/`), written as each version and read back, one run each, the peak resident set of the process after the step (`VmHWM`; the DXF import and all three DWG files are held at once, so the figure is a ceiling):

| Drawing | DXF read | | Write | Read back | Of which the container and objects | Size | Peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mixed 10k (10,000 entities) | 25 ms | `AC1015` | 110 ms | 65 ms | 3 ms, 16,464 objects | 1.12 MB | 45 MB |
| | | `AC1018` | 107 ms | 54 ms | 5 ms | 0.83 MB | 50 MB |
| | | `AC1021` | 121 ms | 60 ms | 5 ms | 0.86 MB | 58 MB |
| mixed 100k (100,000 entities) | 288 ms | `AC1015` | 1,178 ms | 712 ms | 31 ms, 164,064 objects | 11.5 MB | 396 MB |
| | | `AC1018` | 1,142 ms | 707 ms | 50 ms | 8.2 MB | 508 MB |
| | | `AC1021` | 1,265 ms | 714 ms | 52 ms | 8.5 MB | 508 MB |
| lines 100k | 124 ms | `AC1015` | 497 ms | 339 ms | 18 ms | 5.7 MB | — |
| | | `AC1018` | 495 ms | 336 ms | 23 ms | 4.3 MB | — |
| | | `AC1021` | 586 ms | 338 ms | 25 ms | 4.5 MB | — |

The 100,000-entity drawing reads back in 0.71 s in every version, under the ADR's second; the container and the object decoding are a twentieth of that, and the rest is the DXF reader's finishing of the records into the drawing, which the DXF path pays too (288 ms from text). Writing is slower than reading, 1.1–1.3 s at 100k: the records are the DXF exporter's groups, formatted as text and parsed back by the encoders. That is the one walk ADR-0036 chose for correctness; a typed record would halve it, should a drafter notice. The DWG is a third smaller than the DXF (11.5 MB against 32.5 MB of text at 100k; 8.2 MB compressed).

**LibreDWG's samples** (`scripts/dwg-sweep.sh` over its `test-data/2000`, `2004`, and `2007`): 61 opened, 0 refused — 22 of 22, 21 of 21, 18 of 18 — up from 62 of 67 at the spike's count. 22 of them report content the drawing does not hold, as a DXF of the same would: 3D solids, surfaces, meshes, images, PDF underlays, tables, `MLINE`, `HELIX`, `SHAPE`, `SOLID`, `TRACE`, `3DFACE`, OLE, gradient hatches, a `POLYLINE` kind, a group without objects, a text style's name. None is guessed at.

**The suite:** 1,843 Rust tests, 175 web unit tests, 262 end-to-end tests in Chromium and Firefox.

## Decisions

- **A crate of our own, from the specification** (ADR-0036). LibreDWG would have decided the licence and has no R2007 writer; acadrust would have added 1.22 MB gzipped. Both became test oracles instead: acadrust writes the fixtures and reads our output in the tests; LibreDWG's `dwgread` reads both through a script. A disagreement between them was investigated, never averaged — the reader losses of acadrust's DXF (`READER_LOSSES`) are each checked against LibreDWG's reading of the same file.
- **One walk over the drawing.** The DWG writer encodes the DXF exporter's records (`cad_dxf::export_records`), so what a DXF carries a DWG carries, by construction, and the handles are the records' own: the output is deterministic.
- **The specification is followed to the file, not the letter.** Sixteen places where the files disagree with the ODA text are in the matrix's errata table, each confirmed in LibreDWG's `*.spec` and the files. The reader found two of its own while the writer was built: object map offsets reset per section, and `MENUNAME` in the R2007 string stream.
- **`OPEN` reads the bytes, not the extension.** The format comes with the file as session state; `SAVE` keeps it; a save as another format makes it the drawing's.
- **The format is asked at the prompt** with or without a dialog: the dialog's types cannot tell DWG 2000, 2004, and 2007 apart. `DXFOUT` is its own command and writes DXF without a question, as AutoCAD's does; `SAVEAS` and `EXPORT` ask.
- **The ADR's fallback was not needed.** The AC1021 writer was validated against both oracles and the reader on the first run of the full round trip.

## What testing found

- **The reader's object map offsets reset per section** (#315): the spec says nothing; a file with more than one map section was misread until the writer produced one.
- **acadrust wants four shorts, two longs, and a bit after the R2004 header's last handle** (#316), which LibreDWG passes over as unknown; both are written.
- **`MENUNAME` lives in the R2007 string stream** (#317) where the spec stops it at R2004; the reader had shifted every header text after it.
- **LibreDWG's R2007 preview reader takes the sentinels off and reads the count where an image's would be** (#317): the preview now carries sixteen bytes of padding in every version.
- **The stress test found an arc of radius 23,681 whose halves would not join** (#318, `18f5b5d`): the ends' angles rounded a hair past the linear tolerance's angle. `join_arcs` allows for the angles' own rounding; a regression test and proptest's saved case are committed.
- **A drawn spline lost its fit points into DWG** (#319): the writer now carries a spline by its fit points when they define it, so SPLINE round-trips whole; the one case it cannot hold is named before the save.
- **The fuzzer found a page claiming petabytes** (#312, `fuzz-huge-page-blocks.AC1021.dwg`): the Reed–Solomon block count comes from the page on disk, never the claim.

## What follows

**M15 (#231)** holds what needs a person, now with #321: our DWG of each version opened in AutoCAD or DWG TrueView without an audit error, and AutoCAD's own DWG of each version opened here. Nothing else is planned for DWG. Should drafters ask:
- `AC1024` and later as formats to save (reading them is the R2004 container with R2007's object coding);
- `INSERT ~` of a DWG, which today takes a DXF;
- a typed record between the DXF exporter and the DWG encoders, should writing a 100,000-entity drawing in a second matter.
