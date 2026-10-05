# Geometry regression corpus

`intersections.txt` holds intersection cases as plain text, one per line; the format is described at the top of the file. It is run by `crates/cad-geometry/tests/regression.rs`, in both argument orders.

**Rule (DESIGN §21.3):** every fixed geometry bug adds a permanent case here, in the same commit as the fix.

`polyline-offsets.txt` holds polyline offset cases in the same spirit: a polyline, a signed distance, and the number and total length of the paths that should result. The runner (`polyline_offset_corpus` in the same test file) also checks, for every case, that each point of the result is the offset distance from the source — the property the method is built on ([ADR-0011](../../docs/adr/0011-polyline-offset.md)). Run it with `REPORT_OFFSETS=1 ... -- --nocapture` to print what each case produces.
