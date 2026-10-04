# Geometry regression corpus

`intersections.txt` holds intersection cases as plain text, one per line; the format is described at the top of the file. It is run by `crates/cad-geometry/tests/regression.rs`, in both argument orders.

**Rule (DESIGN §21.3):** every fixed geometry bug adds a permanent case here, in the same commit as the fix.
