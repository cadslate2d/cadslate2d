# DXF fixtures

Used by `crates/cad-dxf/tests/fixtures.rs` (DESIGN §41). What the reader and writer do with each record is in [`docs/dxf-matrix.md`](../../docs/dxf-matrix.md), each row naming the fixture or test that backs it.

Fixtures are exercised by directory, so **adding a regression case is a one-file change**:

- `valid/` — every file must import without error, and export → import → export must be stable.
- `malformed/` — every file must be rejected with a structured error. None may panic.

Files with specific expectations (entity counts, report contents, exact coordinates) are also named in individual tests.

## valid/

| File | Origin | What it covers |
| --- | --- | --- |
| `minimal-line.dxf` | hand-written | The smallest useful file: an ENTITIES section with one LINE. |
| `crlf-exponent-notation.dxf` | hand-written | CRLF line endings, exponent and leading-dot numbers, explicit zero elevation. |
| `cadslate-m0-line.dxf` | written by `cad_dxf::export` (golden file) | The M0 user-story line, `0,0` to `@1000<45`, as the R2000 writer puts it: the header, the nine tables, the two spaces, the one line, and the objects ([ADR-0018](../../docs/adr/0018-dxf-release-r2000.md)). Regenerate with `UPDATE_GOLDEN=1 cargo test -p cad-dxf`. |
| `ezdxf-r12-three-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A complete R12 file: header, tables, blocks, three lines. |
| `ezdxf-r2018-two-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2018")` | A modern file (AC1032) with handles, subclass markers, objects, and structural `*Model_Space` blocks. Coordinates chosen to need all 17 significant digits. |
| `ezdxf-r2010-mixed-content.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Mixed content: lines, circles, an arc, a polyline, text, a user block and its insert — all imported now — and lines carrying a layer and a colour. |

| `ezdxf-r2010-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | `LWPOLYLINE` open and closed, one with an arc segment (bulge), an old-style `POLYLINE`, plus a 3D polyline that is reported. |
| `ezdxf-r12-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | R12 `POLYLINE`/`VERTEX`/`SEQEND`, open and closed — the encoding our writer used before ADR-0018. |
| `ezdxf-r12-bulged-polylines.dxf` | ezdxf 1.4.4, `add_polyline2d(..., format="xyb")` | Arc segments as group 42 on `VERTEX` records: a closed slot with two semicircular ends (bulge 1), an open run with a quarter-arc corner (bulge tan 22.5°), and a clockwise arc (bulge −0.5). |
| `ezdxf-r2010-bulged-lwpolylines.dxf` | ezdxf 1.4.4, `add_lwpolyline(..., format="xyb")` | The same three shapes as `LWPOLYLINE`, where group 42 follows the vertex it belongs to. |
| `ezdxf-r12-layers-linetypes.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | `LAYER` and `LTYPE` tables: layers with colours, linetypes, one off and one locked; the DASHED, CENTER, and DOT patterns; entities with a layer, a colour, a linetype of their own, `ByBlock`; `$CLAYER`, `$CECOLOR`, `$CELTYPE`, `$LTSCALE`. |
| `ezdxf-r12-text.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A `STYLE` table with `ROMAN`; `TEXT` left, centred, right (rotated, coloured), middle, top-left, and fit, and one in a style the file never defines. |
| `ezdxf-r2010-mtext.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | `MTEXT`: a paragraph break (`\P`), font, height, colour, and stacking codes, a non-breaking space, attachment points 5 and 9, a rotation, and 300 characters written in pieces (groups 3 then 1). |
| `ezdxf-r2010-properties.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Lineweights (group 370) on a layer, on entities, and in `$CELWEIGHT`; a linetype scale of an entity's own (dropped); a complex linetype with text (dashes kept, shapes reported); a layer and a linetype entities name but the file never defines. |
| `ezdxf-r12-blocks.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A `BLOCKS` section with a chair (a line on layer 0 with a `ByBlock` colour, an arc on its own layer, a coloured circle) and a pair that places the chair twice; `INSERT`s of the pair as it is and scaled, turned, and coloured; of the chair mirrored by a negative X scale and by a negative Y scale; one with a non-uniform scale, one arrayed, and one of a block the file never defines (each reported). |
| `ezdxf-r2010-block-attributes.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | A block with an `ATTDEF`, placed twice, once with an `ATTRIB`, once turned: the definition and the value are imported since #183, and written back the same. |
| `ezdxf-r2010-hatches.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Seven `HATCH`es: solid with a round hole (an edge path of one full arc); ANSI31 at 30° and scale 2 on a polyline path with a bulge; edge paths of lines and an arc, counter-clockwise and clockwise; a spline boundary, a gradient, and the `AR-CONC` pattern (each reported). |
| `ellipses.dxf` | hand-written, R2000 `ELLIPSE` records as ezdxf lays them out | A whole ellipse (centre 50,50, major axis 20 along x, ratio 0.5, parameters 0 to 2π), a quarter arc of one with a vertical major axis, and two that are not ellipses — no axis, a ratio of 1.5 — which are reported. The corpus's `unsupported` case holds the one ellipse ezdxf wrote, imported since #181. |
| `splines.dxf` | hand-written, R2000 `SPLINE` records as ezdxf lays them out | A cubic by four fit points alone, as `add_spline(fit_points)` writes one; the nine-control-point rational quadratic circle with its knots and weights; and a single-point spline, which is not one and is reported. The corpus's `unsupported` case holds the one spline ezdxf wrote, and its `hatches` case a hatch bounded by a spline edge, both imported since #182. |
| `layouts.dxf` | hand-written, R2000 as ezdxf lays it out | Two layouts: `Layout1` with a `VIEWPORT` and a `LINE` on layer `TITLE` in `*Paper_Space`, `Layout2` with a `LINE` in `*Paper_Space0`, and one `LINE` in model space; the `LAYOUT` objects with their plot settings and block records. Kept whole since #184; the lines and the viewport entities of their layouts since #191 and #192, not drawn yet. |
| `text-with-spaces.dxf` | hand-written, from a fuzzer finding (#177) | A `TEXT` whose words begin and end with spaces, and an `MTEXT` in two pieces (3, then 1) where the first piece ends with a space: the spaces at the ends of a value are the words' own, and are kept. |
| `hatch-pattern-origin-far.dxf` | hand-written, from a fuzzer finding (#177) | An ANSI31 `HATCH` whose pattern line is placed at y = 1e20: too far from the region, against the spacing, for its lines to be told apart. Reported as too dense and kept solid. |
| `ezdxf-r2010-dimstyles.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")`; the `DIMPOST` groups (3) added by hand, since ezdxf does not write them | A `DIMSTYLE` table: `ARCH` with every variable the drawing holds set and a `<> mm` suffix, `CIVIL` with a suffix without `<>`, `FLAT` with a text height of zero (reported; the default stands); `$DIMSTYLE` naming `ARCH`. |
| `ezdxf-r2010-groups.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Four `GROUP`s (#214): `DOOR` with a description, a line and an arc; an unnamed one (`*A1`) of a polyline and a line; `NOTE`, not selectable, of a text; and `SOLIDS`, whose only member is a `SOLID` the drawing cannot hold, which is reported. |
| `ezdxf-r2010-frozen-layers.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Layers `FROZEN` (70 bit 1), `OFF` (a negative colour), `BOTH` (frozen and off), and `SHOWN`, a line on each (#213): frozen stays frozen and off stays off through a round trip; only the line on `SHOWN` is shown. |
| `ezdxf-r2010-dimension-chains.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | A `CHAIN` dimension style with `DIMDLI` 7 and a baseline chain of three linear dimensions from the left edge of a rectangle, each line 7 further out (#212): the style's baseline spacing, and where chaining puts each dimension. |
| `ezdxf-r2010-construction-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Two `XLINE`s — one on a `CONSTRUCTION` layer — and two `RAY`s, one coloured, beside a line (#206): construction lines, left out of the extents. |
| `ezdxf-r2010-leaders.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Three `LEADER`s (#205): one in a `NOTES` dimension style with an `MTEXT` note named by handle (340) that becomes its words, one with no note ending leftwards, one with a spline path (72) read straight; and an `MTEXT` paragraph that is nobody's note. |
| `ezdxf-r2010-wide-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | `LWPOLYLINE`s with a constant width (43): a closed rectangle 2 wide, an open run with an arc 1.5 wide, and one whose start and end widths vary from vertex to vertex (`format="xyseb"`), which is dropped and counted (#204). |
| `ezdxf-r12-wide-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | `POLYLINE`s with widths: one from the header's defaults (40, 41), one whose `VERTEX` records all agree, and one whose vertices differ, which is dropped and counted (#204). |
| `ezdxf-r2010-points.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Three `POINT`s — on layer 0, on a layer of their own, coloured — and a line, with `$PDMODE` 35 (an X in a circle) and `$PDSIZE` 2.5 in the header: imported since #198. |
| `ezdxf-r2010-true-colour.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | True colours (group 420): on a layer (with ezdxf's default `62` of 7 beside it), on a line alone, on a line with an index beside it, and pure red; each becomes the nearest index colour and is reported. |
| `huge-vertex-count.dxf` | hand-written | An `LWPOLYLINE` whose count (90) claims 4,294,967,296 vertices and has two: the count is never believed, nothing is allocated for it. |
| `repeated-header.dxf` | hand-written | Every header variable the reader takes, twice: the last wins. |

## corpus/

The round-trip corpus (#155): files written by ezdxf 1.4.4 for four releases — R12, R2000, R2010, R2018 — and every kind of content the drawing holds or reports, each with a `.expect` sidecar saying what CadSlate2D must make of it. A case is two files; **adding a case is adding two files**, no Rust.

The runner is `crates/cad-dxf/tests/corpus.rs` (`cargo test -p cad-dxf --test corpus`). For every `.dxf` it imports the file, checks each line of the sidecar, and then exports, imports, and exports again: the second import must hold nothing unsupported and the second export must be byte for byte the first. A failure is reported by file and line, every failing line listed:

```text
r2000-blocks.dxf:7: mirrored: got "plain", expected "mirrored"
    reference 2 CHAIR 0 50 1 180 mirrored
```

The kinds are curves, polylines, text, MTEXT, layers, blocks, attributes, the header, unsupported entities, degenerate geometry, true colour, hatches (R2000 on, with a gradient from R2010), and dimensions — the last with every kind of dimension the drawing measures, a style, the drafter's text, a turned text, and an ordinate dimension, which is reported (ezdxf writes an aligned dimension as a rotated one, which the sidecars record). A sidecar line is a word and its arguments; `#` starts a comment. The grammar is at the top of `corpus.rs`: counts (`imported lines 3`, `tables layers 2`, `unsupported HATCH 1`, `dropped 2`), the drawing's settings (`unit mm`, `current layer PIPES`, `ltscale 5`), and the i-th entity of a kind in file order (`line 0 0 0 100 0`, `circle 0 50 50 25`, `polyline 1 closed 4`, `bulge 2 0 1`, `text 0 "Left" 2.5`, `reference 2 CHAIR 0 50 1 180 mirrored`), a layer's row (`layer WALLS colour 1 on locked linetype DASHED lineweight 50`), and an entity's properties (`entity 1 layer HIDDEN colour 5 linetype DASHED lineweight 35`).

The files here are generated — `generate.py`, which needs ezdxf (`python3 -m venv .venv && .venv/bin/pip install ezdxf==1.4.4`, then `.venv/bin/python3 test-data/dxf/corpus/generate.py` from the repository root) — so the whole corpus can be rewritten when a kind of content is added; the sidecars are written by the same script, by hand per kind of content, and are the contract. A file saved by an interactive CAD program (AutoCAD, BricsCAD, LibreCAD, QCAD) goes here too, with a sidecar written by hand; none exists yet, for want of someone with the program.

`audit.py` re-exports every corpus file through our writer and reads the result with ezdxf's recovery audit. Recorded 2026-10-06: 52 files, 0 errors, 0 fixes — every export read back as `AC1015` with its entities, and ezdxf reads the geometry of each of our dimensions' `*D` blocks. The check has no Python, so this is run and recorded by hand when the writer changes.

| Kind | Files | What it covers |
| --- | --- | --- |
| `curves` | 4 | Lines, circles, an arc, with exact coordinates |
| `polylines` | 4 | Open, closed, and bulged polylines (`POLYLINE`/`VERTEX` in R12, `LWPOLYLINE` after), and a 3D polyline that is reported |
| `text` | 4 | `TEXT` left, centred (turned, in a style), right, and fit (reported); a `STYLE` row |
| `mtext` | 3 | `MTEXT` with a break, with formatting (reported), and 300 characters in pieces — R2000 and later |
| `layers` | 4 | Layers with colours, linetypes, one off and one locked, lineweights where the release has them; entities with their own properties and `ByBlock` |
| `blocks` | 4 | Nested blocks placed every way: scaled and turned, mirrored by either sign, non-uniform, arrayed, undefined (the last three reported) |
| `header` | 4 | `$CLAYER`, `$CECOLOR`, `$CELTYPE`, `$LTSCALE`; `$INSUNITS` and `$CELWEIGHT` where the release has them |
| `unsupported` | 4 | `POINT`, `SOLID`, `3DFACE`: counted by name; the `ELLIPSE` and `SPLINE` there are imported since #181 and #182 |
| `true-colour` | 2 | Group 420 on a layer and on entities, read as the nearest index and reported — R2010 and R2018, since true colour came with R2004 |
| `attributes` | 4 | A block with an `ATTDEF`, placed with an `ATTRIB` and without: the value read for the first reference, the default shown by the second |
| `mixed` | 4 | A little of everything, counted |
| `degenerate` | 4 | A circle of radius 0, an arc of negative radius, a polyline of one vertex: reported, not imported |

## malformed/

| File | Expected error |
| --- | --- |
| `empty.dxf` | unexpected end |
| `not-dxf.dxf` | invalid group code |
| `bad-group-code.dxf` | invalid group code |
| `bad-coordinate.dxf` | invalid number |
| `line-missing-endpoint.dxf` | missing coordinate |
| `truncated-mid-entity.dxf` | unexpected end |
| `missing-eof.dxf` | unexpected end |
| `binary-sentinel.dxf` | binary DXF not supported |
| `cyclic-blocks.dxf` | tables: block A places B, which places A |
| `long-value.dxf` | value too long: a layer name of 70,000 characters, over the 64 KiB a value may have |
| `self-referencing-block-fanout.dxf` | hand-written, from a CI fuzz finding (#177) | Block `A` places `B`, and `B` places itself forty times: a cycle, refused — and refused at once, where following every reference afresh took forty to the power of the nesting limit. |

## Checking the writer with another reader

`cargo run -p cad-dxf --example reexport -- <file.dxf>` prints a file as CadSlate2D writes it, for reading back with ezdxf or another tool (`ezdxf.recover.readfile` audits it). The exports of `ezdxf-r12-blocks.dxf`, `ezdxf-r2010-properties.dxf`, and `ezdxf-r2010-mtext.dxf` — nested blocks, lineweights, paragraphs — audit clean in ezdxf 1.4.4 as R2000 files, with no fixes ([ADR-0018](../../docs/adr/0018-dxf-release-r2000.md)).

## Not yet covered

There is no file saved by AutoCAD, LibreCAD, QCAD, or another interactive CAD program. The `ezdxf` files stand in for "exported fixtures from established CAD tools"; add real exports here when available.
