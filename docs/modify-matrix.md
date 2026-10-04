# What the modify commands support

M2 implements deliberately narrow cases of the geometry-heavy modify commands (DESIGN §22, §30). This page is the published list of what works. Anything not listed is refused with a message and changes nothing; it is never guessed at. The general cases are M3 work.

Each table is backed by tests named in its section.

## OFFSET

| Object | Result | Refused when |
| --- | --- | --- |
| Line | A parallel line of the same length, on the side of the picked point | The picked point is on the line (it chooses no side) |
| Circle | A concentric circle: larger if the point is outside, smaller if inside | The point is on the circle; or an inward offset would leave nothing |
| Arc | A concentric arc with the same angles: larger if the point is outside the arc's circle, smaller if inside | The point is on the arc's circle; or an inward offset would leave nothing |
| Polyline | Not supported yet | Always: "OFFSET does not support polylines yet." |

The distance must be greater than zero and is remembered for the session.

With the `Through` option there is no distance: after the object is picked, the copy is placed so that it passes through the next point. For a line the distance is measured to the line carried on past its ends, so the point may lie beyond them.

Tests: `cad-geometry/src/offset.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/offset.rs`, `web/e2e/offset.spec.ts`.

## TRIM

Choose the cutting edges like any selection — clicks, a window, a crossing box — and press Enter; or press Enter straight away to make every object an edge. Then each pick removes the part of the picked object around the pick, between the nearest edges on either side.

| Cutting edge ↓ / Object to trim → | Line | Arc | Circle | Open polyline | Closed polyline |
| --- | --- | --- | --- | --- | --- |
| Line | yes | yes | yes, if crossed twice | yes | yes, if crossed twice |
| Circle | yes | yes | yes, if crossed twice | yes | yes, if crossed twice |
| Arc | yes | yes | yes, if crossed twice | yes | yes, if crossed twice |
| Polyline (any of its segments, straight or arc) | yes | yes | yes, if crossed twice | yes | yes, if crossed twice |

Rules:

- The part removed runs from the nearest crossing on one side of the pick to the nearest crossing on the other, whichever edges they belong to, or to the object's end where there is none. A pick between two crossings therefore leaves two objects; the first keeps the original's identity.
- An object can be an edge and a target in the same command: it is cut by the *other* edges.
- A trimmed circle becomes an arc and keeps its identity. It must be crossed at two distinct points, by one edge or by several.
- A crossing at the object's own endpoint divides nothing and is ignored. If no crossing is left: "The object does not cross a cutting edge."
- An edge tangent to a line or arc divides it at the point of contact. A circle touched once cannot be opened: "A circle must be crossed twice by cutting edges to be trimmed."
- Only real crossings count; edges are not extended.
- A polyline is trimmed along its whole length, across vertices, with straight and arc segments alike; a cut part-way along an arc segment leaves arcs with the right bulge on both sides. What is left is one or two polylines. A closed polyline opens into one open polyline and, like a circle, must be crossed at two distinct points: "A closed polyline must be crossed twice by cutting edges to be trimmed."
- A cut exactly at a vertex leaves no segment of zero length.
- A selection made before the command is not taken as the edges.
- While you hover, the part a click would remove is shown in red.

Not implemented: trimming to implied (extended) edges; fence and crossing selection of the objects to trim.

Tests: `cad-geometry/src/trim.rs`, `cad-geometry/src/path.rs`, `cad-document/src/editing.rs` (one test per target kind, each against all four edge kinds, plus tangent, endpoint, vertex, and several-edge cases), `cad-commands/src/tests/edge.rs`, `web/e2e/trim-extend.spec.ts`.

## EXTEND

Choose the boundary edges as for TRIM, or press Enter for all. Then each pick lengthens the picked object, from the end nearer the pick, until it meets a boundary.

| Boundary edge ↓ / Object to extend → | Line | Arc | Open polyline | Circle | Closed polyline |
| --- | --- | --- | --- | --- | --- |
| Line | yes | yes | yes | no | no |
| Circle | yes | yes | yes | no | no |
| Arc | yes | yes | yes | no | no |
| Polyline (any of its segments, straight or arc) | yes | yes | yes | no | no |

Rules:

- A line continues straight on; an arc continues along its own circle, counter-clockwise from its end or clockwise from its start.
- The first meeting with any boundary in that direction is taken. An end already on a boundary goes on to the next meeting, if there is one.
- If the object would never meet a boundary: "The object does not meet a boundary edge when extended." An arc that would come back round to its own other end first counts as never meeting one.
- An open polyline is extended at its first or last segment, whichever end is nearer the pick: a straight segment straight on, an arc segment along its circle.
- Circles and closed polylines have no ends and are refused by name.
- While you hover, the object is shown as a click would leave it.

Not implemented: extending to implied edges.

Tests: `cad-geometry/src/extend.rs`, `cad-geometry/src/path.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/edge.rs`, `web/e2e/trim-extend.spec.ts`.

## FILLET and CHAMFER

Two picks; the two objects are cut back or carried on to a corner and joined.

| First ↓ / Second → | Line | Arc | Circle | Polyline |
| --- | --- | --- | --- | --- |
| Line | yes | no | no | no |
| Arc, Circle, Polyline | no | no | no | no |

Rules:

- The lines need not touch: each is cut back or carried on as required. Where they cross, the side of each line that was picked is the side that is kept.
- **FILLET** joins them with an arc of the set radius, tangent to both. Radius zero makes a sharp corner and no arc.
- **CHAMFER** cuts the first line picked back by the first distance and the second by the second, and joins the two points with a line. Both distances zero makes a sharp corner and no line.
- Both lines keep their identity and direction; the arc or bevel is a new object. One join is one transaction.
- Parallel lines are refused: "The lines are parallel."
- A radius or distance that would use up a whole line is refused: "The radius is too large for these lines." / "The distances are too large for these lines."
- Anything but two lines is refused by name. Picking the same line twice is refused.
- Radius and distances cannot be negative and are remembered for the session.

Not implemented: fillets and chamfers involving arcs, circles, or polylines; the `Polyline`, `Angle`, `Trim`, and `Multiple` options.

Tests: `cad-geometry/src/corner.rs` (including a property test that the fillet arc is tangent to both lines), `cad-document/src/editing.rs`, `cad-commands/src/tests/corner.rs`, `web/e2e/fillet-chamfer.spec.ts`.
