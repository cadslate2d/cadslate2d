# DWG fixtures

Used by `crates/cad-dwg/tests/fixtures.rs` (DESIGN §41, [ADR-0036](../../docs/adr/0036-dwg.md)). What the reader does with each container and record is in [`docs/dwg-matrix.md`](../../docs/dwg-matrix.md), each row naming the fixture or test that backs it.

Fixtures are exercised by directory, so **adding a regression case is a one-file change**:

- `valid/` — every file must open: its sections found, its object map increasing, every object's common data and handles decoded, its classes and header variables read.
- `malformed/` — every file must be refused with the structured error its name says: `truncated-*`, `version-*`, `bad-page-checksum-*`, `bad-header-crc-*`; a `fuzz-*` file, a fuzzer's finding, with any structured error. None may panic.

The writer's output is not a fixture: `crates/cad-dwg/tests/writer.rs` writes every DXF fixture under [`test-data/dxf/valid/`](../dxf/README.md) as an `AC1015` and an `AC1018` DWG at test time and reads it back, through this crate and through acadrust; `scripts/fuzz.sh` seeds the `dwg_read` corpus with the same output, and `scripts/dwg-check.sh` hands it to LibreDWG's `dwgread`.

## valid/

Every valid fixture is written by an independent writer, acadrust 0.6.5, from a DXF fixture under [`test-data/dxf/valid/`](../dxf/README.md), in the three versions, by `crates/cad-dwg/examples/fixtures.rs`:

```bash
cargo run -p cad-dwg --example fixtures -- minimal-line ezdxf-r2010-mixed-content
```

The source goes through this project's own DXF reader and writer first, so that every row an entity names is defined: a DWG holds rows by handle and cannot say a name the file never defines, which the writer would drop. `crates/cad-dwg/tests/drawing.rs` compares the drawing each fixture makes with the drawing that normalised source makes, in all three versions (`every_fixture_makes_the_drawing_its_dxf_source_makes()`); what the writer cannot carry — a true colour or a multileader style into `AC1015`, a style's block scale — is listed in that test (`WRITER_LOSSES`), each checked against LibreDWG's reading of the same file. Run the generator deliberately, like the screenshot baselines, and look at what the tests make of the result. A file is named `<dxf fixture>.<version>.dwg`.

| Source | What it covers |
| --- | --- |
| `minimal-line.dxf` | The smallest drawing: one line on layer 0, with the tables, blocks, and dictionaries every DWG carries. |
| `ezdxf-r2010-mixed-content.dxf` | Lines, circles, an arc, a lightweight polyline, text, a user block and its insert, layers and colours; and the classes a modern writer defines. |
| `ezdxf-r12-three-lines.dxf`, `ezdxf-r2018-two-lines.dxf` | Lines alone, from the oldest and the newest source; coordinates that need every digit. |
| `ezdxf-r12-layers-linetypes.dxf`, `ezdxf-r2010-frozen-layers.dxf` | Layers with colours, linetypes, one off, one frozen, one locked; the DASHED, CENTER, and DOT patterns; entities with a layer, a colour, a linetype of their own, `ByBlock`. |
| `ezdxf-r2010-properties.dxf`, `ezdxf-r2010-true-colour.dxf` | Lineweights on a layer and on entities; a linetype scale; a complex linetype; true colours on a layer and on entities (`AC1018` and `AC1021` hold them; `AC1015` takes the writer's nearest index). |
| `ezdxf-r12-text.dxf`, `ezdxf-r2010-text-styles.dxf`, `ezdxf-r2010-mtext.dxf` | `TEXT` in every justification, rotated, with a width factor and a slant; `STYLE` rows with a fixed height, a width factor, and a slant; `MTEXT` with formatting codes, attachment points, a rotation, and 300 characters. |
| `ezdxf-r12-blocks.dxf`, `ezdxf-r2010-block-attributes.dxf` | Blocks nested two deep, placed as they are and scaled, turned, mirrored, arrayed; a block with an `ATTDEF`, placed with an `ATTRIB`. |
| `ezdxf-r12-polylines.dxf`, `ezdxf-r12-bulged-polylines.dxf`, `ezdxf-r12-wide-polylines.dxf`, `ezdxf-r2010-polylines.dxf`, `ezdxf-r2010-bulged-lwpolylines.dxf`, `ezdxf-r2010-wide-polylines.dxf` | `POLYLINE` with its `VERTEX` chain and `SEQEND`, 2D and 3D, open and closed, with bulges and widths; `LWPOLYLINE` likewise. |
| `ezdxf-r2010-points.dxf`, `ezdxf-r2010-construction-lines.dxf` | `POINT`s and the point style; `XLINE`s and `RAY`s. |
| `ezdxf-r2010-hatches.dxf`, `ezdxf-r2010-draw-order.dxf` | `HATCH` solid, patterned, with polyline paths, edge paths of lines and arcs, a spline edge; a `WIPEOUT`, with the drawing order (`SORTENTSTABLE`). |
| `ezdxf-r2010-dimstyles.dxf` | `DIMSTYLE` rows with every variable the drawing holds and a `DIMPOST` suffix. |
| `ezdxf-r2010-dimension-chains.dxf`, `ezdxf-r2010-ordinate-dimensions.dxf` | `DIMENSION` of every kind: linear, aligned, angular, radius, diameter, ordinate; with text overrides and the blocks they own. |
| `ezdxf-r2010-leaders.dxf`, `ezdxf-r2010-multileaders.dxf` | `LEADER`s; `MULTILEADER`s with text and with a block, and their `MLEADERSTYLE`s (`AC1018` and `AC1021` hold the styles; `AC1015` predates them, and the writer leaves a style's block scale at 1). |
| `layouts.dxf` | Two layouts, each with a line on its paper, and a `VIEWPORT` on the first. The writer names both paper blocks `*Paper_Space`; the reader names the second `*Paper_Space0`, as AutoCAD does. |
| `ezdxf-r2010-named-views.dxf`, `ezdxf-r2010-groups.dxf`, `ezdxf-r2010-layer-states.dxf` | `VIEW` rows; `GROUP`s named and unnamed; `ACAD_LAYERSTATES` `XRECORD`s. |

Three fixtures are crafted from `ezdxf-r2010-draw-order.AC1015.dwg` by rewriting one entity's type and its CRC (the `craft` tool of #313's working notes; the result is checked by the tests, not by hand):

| File | What it covers |
| --- | --- |
| `crafted-proxy-entity.AC1015.dwg` | The `WIPEOUT` entity's type rewritten to `0x1F2`, a proxy entity: reported by its kind, never guessed at. |
| `crafted-unknown-class.AC1015.dwg` | The same entity's type rewritten to 999, a class the drawing does not define: reported, never guessed at. |
| `crafted-mline-entity.AC1015.dwg` | The same entity's type rewritten to `0x2F`, an `MLINE`, a fixed type this crate does not decode: reported as `MLINE` (#314). |

## malformed/

| File | Made from | What it covers |
| --- | --- | --- |
| `truncated.AC1015.dwg` | The first half of `minimal-line.AC1015.dwg` | A file that ends inside its objects. |
| `truncated.AC1021.dwg` | The first 0x500 bytes of `minimal-line.AC1021.dwg` | A file that ends inside its page map. |
| `version-2010.dwg` | `minimal-line.AC1015.dwg` with its id changed to `AC1024` | A version this crate does not read, refused by name. |
| `bad-page-checksum.AC1018.dwg` | `minimal-line.AC1018.dwg` with one byte flipped in the middle | A page whose stored checksum no longer matches its data. |
| `bad-header-crc.AC1015.dwg` | `minimal-line.AC1015.dwg` with one byte flipped in its section locators | A file header whose CRC no longer matches. |
| `fuzz-huge-page-blocks.AC1021.dwg` | The `dwg_read` fuzzer, from `minimal-line.AC1021.dwg` (#312) | A section map whose page claims a compressed size of petabytes: the Reed–Solomon block count must come from the page on disk, never from the claim. |
