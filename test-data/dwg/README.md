# DWG fixtures

Used by `crates/cad-dwg/tests/fixtures.rs` (DESIGN §41, [ADR-0036](../../docs/adr/0036-dwg.md)). What the reader does with each container and record is in [`docs/dwg-matrix.md`](../../docs/dwg-matrix.md), each row naming the fixture or test that backs it.

Fixtures are exercised by directory, so **adding a regression case is a one-file change**:

- `valid/` — every file must open: its sections found, its object map increasing, every object's common data and handles decoded, its classes and header variables read.
- `malformed/` — every file must be refused with the structured error its name says: `truncated-*`, `version-*`, `bad-page-checksum-*`, `bad-header-crc-*`; a `fuzz-*` file, a fuzzer's finding, with any structured error. None may panic.

## valid/

Every valid fixture is written by an independent writer, acadrust 0.6.5, from a DXF fixture under [`test-data/dxf/valid/`](../dxf/README.md), in the three versions, by `crates/cad-dwg/examples/fixtures.rs`:

```bash
cargo run -p cad-dwg --example fixtures -- minimal-line ezdxf-r2010-mixed-content
```

Run it deliberately, like the screenshot baselines, and look at what the tests make of the result. A file is named `<dxf fixture>.<version>.dwg`.

| Source | What it covers |
| --- | --- |
| `minimal-line.dxf` | The smallest drawing: one line on layer 0, with the tables, blocks, and dictionaries every DWG carries. |
| `ezdxf-r2010-mixed-content.dxf` | Lines, circles, an arc, a lightweight polyline, text, a user block and its insert, layers and colours; and the classes a modern writer defines. |

## malformed/

| File | Made from | What it covers |
| --- | --- | --- |
| `truncated.AC1015.dwg` | The first half of `minimal-line.AC1015.dwg` | A file that ends inside its objects. |
| `truncated.AC1021.dwg` | The first 0x500 bytes of `minimal-line.AC1021.dwg` | A file that ends inside its page map. |
| `version-2010.dwg` | `minimal-line.AC1015.dwg` with its id changed to `AC1024` | A version this crate does not read, refused by name. |
| `bad-page-checksum.AC1018.dwg` | `minimal-line.AC1018.dwg` with one byte flipped in the middle | A page whose stored checksum no longer matches its data. |
| `bad-header-crc.AC1015.dwg` | `minimal-line.AC1015.dwg` with one byte flipped in its section locators | A file header whose CRC no longer matches. |
| `fuzz-huge-page-blocks.AC1021.dwg` | The `dwg_read` fuzzer, from `minimal-line.AC1021.dwg` (#312) | A section map whose page claims a compressed size of petabytes: the Reed–Solomon block count must come from the page on disk, never from the claim. |
