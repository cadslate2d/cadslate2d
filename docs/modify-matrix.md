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

## TRIM

One cutting edge; then each pick removes the part of the picked object on the picked side of the edge.

| Cutting edge ↓ / Object to trim → | Line | Arc | Circle | Polyline |
| --- | --- | --- | --- | --- |
| Line | yes | yes | yes, if crossed twice | no |
| Circle | yes | yes | yes, if crossed twice | no |
| Arc | yes | yes | yes, if crossed twice | no |
| Polyline (any of its segments) | yes | yes | yes, if crossed twice | no |

Rules:

- The part removed runs from the nearest crossing on one side of the pick to the nearest crossing on the other, or to the object's end where there is none. A pick between two crossings therefore leaves two objects; the first keeps the original's identity.
- A trimmed circle becomes an arc and keeps its identity.
- A crossing at the object's own endpoint divides nothing and is ignored. If no crossing is left: "The object does not cross the cutting edge."
- A cutting edge tangent to a line or arc divides it at the point of contact. A circle touched once cannot be opened: "A circle must cross the cutting edge twice to be trimmed."
- Only real crossings count; the cutting edge is not extended.
- Polylines cannot be trimmed yet: "TRIM does not support polylines yet."

Not implemented: several cutting edges at once, trimming to implied (extended) edges, fence selection.

Tests: `cad-geometry/src/trim.rs`, `cad-document/src/editing.rs` (one test per target kind, each against all four edge kinds, plus tangent and endpoint regressions), `cad-commands/src/tests/edge.rs`, `web/e2e/trim-extend.spec.ts`.

## EXTEND

One boundary edge; then each pick lengthens the picked object from the end nearer the pick until it meets the edge.

| Boundary edge ↓ / Object to extend → | Line | Arc | Circle | Polyline |
| --- | --- | --- | --- | --- |
| Line | yes | yes | no | no |
| Circle | yes | yes | no | no |
| Arc | yes | yes | no | no |
| Polyline (any of its segments) | yes | yes | no | no |

Rules:

- A line continues straight on; an arc continues along its own circle, counter-clockwise from its end or clockwise from its start.
- The first meeting with the boundary in that direction is taken. An end already on the boundary goes on to the next meeting, if there is one.
- If the object would never meet the boundary: "The object does not meet the boundary edge when extended." An arc that would come back round to its own other end first counts as never meeting it.
- Circles have no ends, and polylines are not supported yet; both are refused by name.

Not implemented: several boundary edges at once, extending to implied edges.

Tests: `cad-geometry/src/extend.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/edge.rs`, `web/e2e/trim-extend.spec.ts`.
