# M5 exit review — DXF Compatibility and Local Persistence

**Date:** 2026-10-06  
**Scope:** DESIGN §33, as rewritten at the M4 exit ([roadmap](../roadmap.md))  
**Outcome:** Every item of the rewritten M5 scope is implemented and tested, an automated scenario exercises them together, and the milestone is closed. M5 changed what a saved drawing is: the writer moved from R12 to R2000 and nothing the drawing holds is lost on save any more; what the reader does with every record is one page backed by tests; a corpus of 45 files from another writer round-trips byte for byte; every quantity a hostile file can grow is bounded; what an import skipped is shown and a lossy export asks first; and the drawing is kept in the browser and offered back after a crash, saved back to its file where the browser can. Two things still need a person: an interactive CAD program reading our files, and a real browser with the File System Access API in a human's hands.

§33 has no exit sentence of its own. The epic's "done when" list stood in for one, and is what the criteria below are checked against.

## The scenario

`web/e2e/m5-scenario.spec.ts` saves a drawing whole and keeps it safe, in Chromium and Firefox:

1. A layer with a colour, a linetype, and a lineweight is made current; a line, a circle, an arc, a polyline with an arc segment, a text, a paragraph typed in the editor, and a chair made a block and inserted scaled and turned: **every M4 table and entity**.
2. `DXFOUT` writes R2000: the layer row carries its lineweight, the paragraph is one `MTEXT` with its break as a code, the polyline an `LWPOLYLINE`, the block a definition and two `INSERT`s.
3. The file is reopened — "Imported 1 line, 1 circle, 1 arc, 1 polyline, 2 texts, 2 block references.", "With 1 layer, 1 linetype, and 1 block.", no `LOSSY` — and saved again: **the same bytes**.
4. A hostile file is refused with its reason — a 70,000-character value, a cycle of blocks — and the drawing stays what it was, byte for byte.
5. A file with content the drawing cannot hold is opened: the **import report** names it on screen, the status bar says `LOSSY`, and `DXFOUT` **asks first**, naming what the file will lose; the user cancels.
6. A change is made and the tab reloaded: the **autosave** offers the drawing back under its file's name; recovered and exported, it holds the change and everything that came before.
7. In Chromium, with the file pickers faked: `SAVE` asks where once, then **writes back** to the same file with no dialog.

## Done when (epic #104)

| Criterion | Where | Evidence |
| --- | --- | --- |
| The writer's release holds lineweights and paragraphs, by an ADR with evidence; export → import → export is byte-identical for every fixture | `cad-dxf/src/export.rs`; [ADR-0018](../adr/0018-dxf-release-r2000.md) | `everything_the_drawing_holds_survives_export_import_export_byte_for_byte`, `a_paragraph_is_written_as_mtext_with_breaks_and_codes_escaped_and_survives`; `every_valid_fixture_round_trips_stably` over 19 fixtures; ezdxf 1.4.4 reads three exports with 0 audit errors and 0 fixes |
| One page says what every DXF record, table, and header variable becomes, each row backed by a test | [`docs/dxf-matrix.md`](../dxf-matrix.md) | `the_matrix_names_only_tests_and_fixtures_that_exist` reads the page and fails on a name that does not exist (82 names checked) |
| The fixture corpus spans four releases and takes a new case as a file and a sidecar | `test-data/dxf/corpus/`, `cad-dxf/tests/corpus.rs` | 45 files from ezdxf for R12, R2000, R2010, R2018; `every_corpus_file_imports_as_its_sidecar_says_and_round_trips`; `audit.py`: 45 exports, 0 errors, 0 fixes |
| Every quantity a file can grow is bounded, with a test that reaches the bound; a fuzz run finds no panic | `ImportLimits`, `cad-dxf/src/import.rs` | Six bounds, each with a test; `a_flood_of_empty_blocks_stops_at_the_row_limit`; the time budget (100 MB in 0.37 s, release); `arbitrary_records_of_every_kind_never_panic_and_round_trip` — 20,000 cases, 13 s, clean |
| What an import skipped is shown, and a lossy export warns with what it will lose | `web/src/ui/{ImportReport,ExportWarning}.tsx`, `UiState.export_losses` | `cad-app.test.ts` "is reported on import and confirmed before export"; `web/e2e/files.spec.ts` |
| A reload recovers the drawing from an autosave; where the browser allows, SAVE writes back to the opened file | `web/src/app/{cad-app,host-services}.ts`; [ADR-0019](../adr/0019-local-persistence.md) | `cad-app.test.ts` "autosave and recovery" and "saving to a file" (both capabilities); `web/e2e/{autosave,file-handles}.spec.ts` |

Constraints, as the epic set them:

| Constraint | How it was kept |
| --- | --- |
| Unsupported DXF content is counted, reported, and warned about before lossy export (§24.2, LOCKED) | The report keys are enumerated in the matrix; `ImportReport::losses` is what the warning names; the status bar marks the drawing while an export would lose something |
| No Worker solely for synchronous OPFS access (§25.1, §25.2) | The autosave is IndexedDB on the main thread; ADR-0019 measured OPFS's async API too and rejected it for Safari's main thread, not for speed |
| Browsers without writable handles still get a polished import/autosave/export workflow (§25.3) | One `FileAccess` interface, two implementations chosen by capability; Firefox's path is the download, the same autosave and recovery |
| No account, no drawing upload, no drawing-content telemetry (§43) | The autosave scenario records every request the page makes: the app's own files from its own origin and the download's `blob:` URL, nothing else |
| Dialogs stay narrow and task-specific (§5); React holds no drawing data | Three dialogs were added — the export warning, the recovery offer, and (unchanged) the paragraph editor — each a question and two buttons; the panels and dialogs show lines and names from `UiState` |

Test counts at this review: 1262 Rust tests, 128 Vitest tests, 370 Playwright tests across Chromium and Firefox (the file-handle tests run in Chromium only; the visual baselines are compared there only). At the M4 review they were 1242, 119, and 359.

## What was built

| Issue | What |
| --- | --- |
| #152 | The R2000 writer: handles, the nine tables, both spaces, the objects; lineweights, MTEXT, LWPOLYLINE — [ADR-0018](../adr/0018-dxf-release-r2000.md). The e2e suite reads exports through a record parser instead of matching R12 substrings |
| #153 | True colour not adopted; a file's group 420 is read as the nearest index and reported |
| #154 | [`docs/dxf-matrix.md`](../dxf-matrix.md), every row backed by a named test or fixture, checked by a test |
| #155 | The 45-file corpus with one-line-per-expectation sidecars, its generator, its ezdxf audit |
| #156 | `ImportLimits` for rows, record groups, value bytes, and nesting depth; tables indexed by name; the time budget and the structured fuzz in `scripts/stress.sh` |
| #157 | The import report panel, the export warning that names the losses, the `LOSSY` marker; `UiState.export_losses` and `import_report` |
| #158 | The autosave in IndexedDB and the recovery offer — [ADR-0019](../adr/0019-local-persistence.md); `UiState.revision`, `snapshot_dxf` |
| #159 | `SAVE` as its own command and host request; `FileAccess` with the File System Access and download implementations; the file name and unsaved marker in the title and the status bar |

Beyond the list: `Drawing::from_tables` takes blocks and checks cycles after inserting them all (#149's reader needed it); `BlockTable::nesting_depth`; the e2e harness removes the file pickers before the app starts, or fakes them.

## Decisions made in M5

| Item | Decision |
| --- | --- |
| The DXF release (§24.1, PROVISIONAL) | R2000 and only R2000; a complete file, no larger than that; handles follow entity IDs — [ADR-0018](../adr/0018-dxf-release-r2000.md). R12 is read, not written |
| True colour | Not adopted: the palette is what command-driven drafting types. A file's true colour becomes the nearest index, by distance in RGB over the palette's own table, and is reported; the `62` beside it is not trusted (ezdxf writes a default there) |
| Local persistence (§25.1, PROVISIONAL) | One IndexedDB record — the DXF, the name, the time, exported or not — written a second after the revision last changed; offered back on start when no file holds it — [ADR-0019](../adr/0019-local-persistence.md) |
| Who decides when to autosave | The host, from `UiState.revision`, not a `HostRequest`: the autosave is the host's policy about the host's storage, and the request slot stays a command's |
| Files (§25.3) | `FileAccess` chosen by capability: write-back where `showOpenFilePicker`/`showSaveFilePicker` exist, downloads elsewhere; `SAVE` asks where once and again if the browser refuses; `SAVEAS`, `DXFOUT`, `EXPORT` always ask |
| What "lossy" is | Decided in Rust: `ImportReport::losses`, the summary lines that name content the drawing could not hold; the shell shows them in the report, the warning, and the marker |
| Limits (§43) | Bytes 256 MB, entities 2,000,000, rows per table 100,000, groups per record 1,000,000, bytes per value 64 KiB, nesting 32 — an error or a report line at the bound, never an allocation for a claimed count |
| The corpus's form | A `.dxf` and a `.expect` sidecar, one expectation per line, failures by file and line — the geometry corpora's form; generated by a checked-in script, with the sidecars as the contract |

## What testing found

| Found by | Defect | Fix |
| --- | --- | --- |
| The corpus's `true-colour` case for R2000, which came in without colours | ADR-0018 and the issue said R2000 brought true colour; it came with R2004, and ezdxf writes no group 420 into an R2000 file | The ADR corrected; the case moved to R2010 and R2018 |
| The cyclic-blocks fixture, which was *not* refused | The reader took the block names out of its table before resolving the references inside definitions, so nested references were dropped as undefined and the cycle could not be seen (#149, found while finishing it in M5) | Names stay in the table while each definition is built |
| The never-panics property test, once the writer wrote a viewport | A drawing with coordinates near 10³⁰⁸ gave a viewport height of infinity, which the reader rightly rejected as a number | The centre and height are computed in halves and fall back when not finite |
| The matrix checker's first run | The page's own illustrative `a_test_name()` was a name that did not exist | The checker does its job; the intro was reworded |
| The generated corpus | ezdxf will not write an INSERT with a scale of zero: the degenerate case's reference came out whole | The sidecar says so; the zero-scale case stays in the unit tests |
| A session test comparing two `UiState`s | `revision` is a process-wide counter, so two fresh sessions differ | The test compares everything but the revision |
| Headless Chromium, every spec that saved | Chromium has `showSaveFilePicker`; the app chose write-back and opened a native dialog no test could answer, and every download timed out | The harness removes the pickers before the app starts, or fakes them for the one spec that wants them |
| Vitest, importing `host-services.ts` | `window` was read at module load to choose the file path | The choice is made on first use |
| Firefox, under full parallel load | The canvas is intermittently not visible at load, as at the M4 review | Passes on rerun; tracked with #109 |

## Measurements

| What | Result |
| --- | --- |
| R12 vs R2000, the nested-blocks fixture re-exported | 1.4 KB vs 6.6 KB; a line is eleven groups instead of five |
| ezdxf 1.4.4 reading our R2000 output | Three fixtures and all 45 corpus files: 0 audit errors, 0 fixes |
| The reader on a 100 MB file of empty groups | 0.37 s (release); 10 MB within a minute in debug |
| A flood of 150,000 empty blocks | Refused at the 100,000-row bound in under a second; the tables are indexed by name |
| The structured fuzz | 20,000 files of every record kind: 13 s, no panic, every import round-trips |
| Writing the 100k-line drawing (16.4 MB) to browser storage, main thread (ADR-0019) | IndexedDB 38 ms in Chromium, 113 ms in Firefox; OPFS 33 and 36 ms; the longest gap between frames during any write 23 ms |

**Not established**: WebKit for any of it (not installed here); an interactive CAD program reading our files; the main-thread cost of producing a 16 MB DXF for the autosave, which is the export's and would be the Worker's case (§37).

## What M5 taught us

- **A complete file is cheaper than a clever one.** The pruning experiment showed ezdxf accepts an R2000 file with no `OBJECTS` section at all; the writer writes the whole structure anyway, for a few hundred bytes, because the lenient reader is the one we can run.
- **The honest report had become the product's most-read line.** Four milestones of "lineweights are not written in DXF R12" were right and were not a fix. The release decision was due when the first thing R12 could not hold shipped; it waited for the issue that said so.
- **Sidecars make a corpus a contract.** Writing the expectations by hand per kind of content found two facts the code did not know — true colour's release, ezdxf's refusal of a zero scale — before any test ran.
- **Index the tables.** A reader linear in its rows was quadratic in a file of a hundred thousand of them. Hostile-file thinking found it; a `HashMap` fixed it; the bound makes it moot anyway.
- **Measure both candidates, then choose on the axis that is not speed.** IndexedDB and OPFS were a frame apart; Safari's main thread decided it.
- **Capabilities, not user agents, and the test harness must know the capabilities too.** Chromium's save picker hung fifty tests until the harness removed it; the fix was one init script and the rule that the app chooses by what exists.
- **The host owns its policy.** The autosave's "when" is the host's, read from a revision; the issue's `HostRequest` would have put a browser's storage policy into a command's request slot.
- **The DOM typings lag the platform.** `showSaveFilePicker` and `queryPermission` needed a local declaration; the compiler then caught every optional call.

## Open items

Needs a person:

1. **An interactive CAD program reading our R2000 files** — AutoCAD, BricsCAD, LibreCAD, QCAD — with blocks, lineweights, paragraphs, and `$INSUNITS`. ezdxf reads them clean; nothing with a screen has been tried. One file saved by such a program belongs in the corpus too.
2. **The File System Access path in a real browser**: a human opening a file, editing, pressing Ctrl+S, and finding the file changed; the permission prompt the browser shows on write-back; the handle surviving a reload. The e2e fakes the pickers.
3. **WebKit** for the persistence measurement and the e2e suite (#109).
4. Still open from before: how M3 and M4 feel against classic CAD ([M3 review](m3-exit-review.md), [M4 review](m4-exit-review.md)); the hands-on assessment of the M2 exit condition (#101); the arc grip behaviour (ADR-0010); render measurements on real GPU hardware (#110); whether to report the R-tree bulk-load crash upstream (#111).

Carried forward as issues: drafting far from the origin end to end (#151); Firefox in CI and any WebKit run (#109).

Known limitations, accepted for now:

- R2000 only on the way out; no R12 option. Entities the model lacks — dimensions, hatches, splines, ellipses, points — are counted and reported, not imported; dimensions and hatches are M6.
- One autosave slot, written as DXF, a second after the last change; producing the DXF for a 100,000-entity drawing is main-thread work.
- The export warning and the import report are the §24.2 behaviour; opaque passthrough of unsupported records stays FUTURE.
- No native project format (§39); no cloud storage of any kind.

## M6, rewritten

§34 lists candidate work for annotation and hatch in a dependency order: stable text metrics, then the dimension layout engine, then dimension rendering, then hatch. With M5 done, the order is ([`docs/roadmap.md`](../roadmap.md) has the reasoning):

1. **Text metrics for dimensions** (#161): the functions the layout engine may rely on, named and held under property tests. §34's "improved text renderer" is not taken up: ADR-0016's stroke font stands until #110 measures it on hardware.
2. **Dimension styles** (#162): the table, `DIMSTYLE`, and the DXF `DIMSTYLE` table both ways.
3. **The dimension entity and its layout engine** (#163), with an ADR: definition points and a style, the geometry computed when asked for, as a block reference's is.
4. **`DIMLINEAR` and `DIMALIGNED`** (#164); **`DIMANGULAR`, `DIMRADIUS`, `DIMDIAMETER`** (#165); **dimensions in DXF** (#166).
5. **Hatch**: the entity, solid fill, and `HATCH`, with an ADR on how a filled region reaches the GPU (#167); **patterns** (#168); **hatches in DXF** (#169).
6. **Exit** (#170).
