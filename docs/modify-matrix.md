# What the modify commands support

M2 implements deliberately narrow cases of the geometry-heavy modify commands (DESIGN §22, §30). This page is the published list of what works. Anything not listed is refused with a message and changes nothing; it is never guessed at. The general cases are M3 work.

Each table is backed by tests named in its section.

## OFFSET

| Object | Result | Refused when |
| --- | --- | --- |
| Line | A parallel line of the same length, on the side of the picked point | The picked point is on the line (it chooses no side) |
| Circle | A concentric circle: larger if the point is outside, smaller if inside | The point is on the circle; or an inward offset would leave no circle |
| Arc | Not supported yet | Always: "OFFSET does not support arcs yet." |
| Polyline | Not supported yet | Always: "OFFSET does not support polylines yet." |

The distance must be greater than zero and is remembered for the session. Not implemented: the `Through` option.

Tests: `cad-geometry/src/offset.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/offset.rs`, `web/e2e/offset.spec.ts`.
