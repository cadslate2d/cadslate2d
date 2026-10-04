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
| `cadslate-m0-line.dxf` | written by `cad_dxf::export` (golden file) | The M0 user-story line, `0,0` to `@1000<45`. Regenerate with `UPDATE_GOLDEN=1 cargo test -p cad-dxf`. |
| `ezdxf-r12-three-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R12")` | A complete R12 file: header, tables, blocks, three lines. |
| `ezdxf-r2018-two-lines.dxf` | ezdxf 1.4.4, `ezdxf.new("R2018")` | A modern file (AC1032) with handles, subclass markers, objects, and structural `*Model_Space` blocks. Coordinates chosen to need all 17 significant digits. |
| `ezdxf-r2010-mixed-content.dxf` | ezdxf 1.4.4, `ezdxf.new("R2010")` | Mixed content: lines, circles, and an arc (imported), plus a polyline, text, a user block and its insert (unsupported), and lines carrying a layer and a colour (properties dropped). |

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

## Not yet covered

There is no file saved by AutoCAD, LibreCAD, QCAD, or another interactive CAD program. The `ezdxf` files stand in for "exported fixtures from established CAD tools"; add real exports here when available.
