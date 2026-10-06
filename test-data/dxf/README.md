# DXF fixtures

Used by `crates/cad-dxf/tests/fixtures.rs` (DESIGN §41).

Fixtures are exercised by directory, so **adding a regression case is a one-file change**:

- `valid/` — every file must import without error, and export → import → export must be stable.
- `malformed/` — every file must be rejected with a structured error. None may panic.

Files with specific expectations (entity counts, report contents, exact coordinates) are also named in individual tests.

## valid/

| File | Origin | What it covers |
| --- | --- | --- |
| `minimal-line.dxf` | hand-written | The smallest useful file: an ENTITIES section with one LINE. |
| `crlf-exponent-notation.dxf` | hand-written | CRLF line endings, exponent and leading-dot numbers, explicit zero elevation. |
| `cadslate-m0-line.dxf` | written by `cad_dxf::export` (golden file) | The M0 user-story line, `0,0` to `@1000<45`, with the `TABLES` section every export has. Regenerate with `UPDATE_GOLDEN=1 cargo test -p cad-dxf`. |
| `ezdxf-r12-three-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A complete R12 file: header, tables, blocks, three lines. |
| `ezdxf-r2018-two-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2018")` | A modern file (AC1032) with handles, subclass markers, objects, and structural `*Model_Space` blocks. Coordinates chosen to need all 17 significant digits. |
| `ezdxf-r2010-mixed-content.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Mixed content: lines, circles, an arc, and a polyline (imported), plus text, a user block and its insert (unsupported), and lines carrying a layer and a colour (properties dropped). |

| `ezdxf-r2010-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | `LWPOLYLINE` open and closed, one with an arc segment (bulge), an old-style `POLYLINE`, plus a 3D polyline that is reported. |
| `ezdxf-r12-polylines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | R12 `POLYLINE`/`VERTEX`/`SEQEND`, open and closed — the encoding our own writer uses. |
| `ezdxf-r12-bulged-polylines.dxf` | ezdxf 1.4.4, `add_polyline2d(..., format="xyb")` | Arc segments as group 42 on `VERTEX` records: a closed slot with two semicircular ends (bulge 1), an open run with a quarter-arc corner (bulge tan 22.5°), and a clockwise arc (bulge −0.5). |
| `ezdxf-r2010-bulged-lwpolylines.dxf` | ezdxf 1.4.4, `add_lwpolyline(..., format="xyb")` | The same three shapes as `LWPOLYLINE`, where group 42 follows the vertex it belongs to. |
| `ezdxf-r12-layers-linetypes.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | `LAYER` and `LTYPE` tables: layers with colours, linetypes, one off and one locked; the DASHED, CENTER, and DOT patterns; entities with a layer, a colour, a linetype of their own, `ByBlock`; `$CLAYER`, `$CECOLOR`, `$CELTYPE`, `$LTSCALE`. |
| `ezdxf-r12-text.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A `STYLE` table with `ROMAN`; `TEXT` left, centred, right (rotated, coloured), middle, top-left, and fit, and one in a style the file never defines. |
| `ezdxf-r2010-mtext.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | `MTEXT`: a paragraph break (`\P`), font, height, colour, and stacking codes, a non-breaking space, attachment points 5 and 9, a rotation, and 300 characters written in pieces (groups 3 then 1). |
| `ezdxf-r2010-properties.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Lineweights (group 370) on a layer, on entities, and in `$CELWEIGHT`; a linetype scale of an entity's own (dropped); a complex linetype with text (dashes kept, shapes reported); a layer and a linetype entities name but the file never defines. |

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

## Checking the writer with another reader

`cargo run -p cad-dxf --example reexport -- <file.dxf>` prints a file as CadSlate2D writes it, for reading back with ezdxf or another tool (`ezdxf.recover.readfile` audits it). The export of `ezdxf-r12-layers-linetypes.dxf` audits clean in ezdxf 1.4.4.

## Not yet covered

There is no file saved by AutoCAD, LibreCAD, QCAD, or another interactive CAD program. The `ezdxf` files stand in for "exported fixtures from established CAD tools"; add real exports here when available.
