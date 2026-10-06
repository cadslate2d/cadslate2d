# What the modify commands support

This page is the published list of what each modify command does to each kind of object (DESIGN §22, §30, §31). Anything not listed is refused with a message and changes nothing; it is never guessed at. Each section names the tests that back it.

Two rules hold for every command here:

- **A command returns its result or a reason.** The operation underneath is a pure function that produces the new geometry or says, as a typed value, why not; the command turns that reason into the message quoted in its section. A refusal leaves the drawing exactly as it was.
- **One committed action is one undo step**, and an object that is changed rather than replaced keeps its identity.

## What happens to an object's properties

Every object has a layer, a colour, a linetype, and a lineweight beside its geometry ([ADR-0013](adr/0013-entity-properties-and-tables.md)). No modify command changes them except by this rule:

| The object is… | Its properties |
| --- | --- |
| Changed: moved, rotated, scaled, mirrored, stretched, trimmed, extended, broken, filleted, chamfered, or edited by a grip | Its own, unchanged |
| Made from another: a copy, a mirror image kept beside its original, an offset, a further piece of a trim or a break | Those of the object it was made from |
| A fillet arc or a chamfer bevel | Those of the object picked first |
| Joined from several | Those of its oldest member, whose identity it keeps |
| Newly drawn | Those new objects are given: the current layer (`LAYER`, `Set`), colour (`COLOR`), linetype (`LINETYPE`, `Set`), and lineweight (`LWEIGHT`) |
| `CHPROP` | Gives every selected object the layer, colour, linetype, or lineweight named, keeping the rest; one transaction per property, one undo for the command. Objects on a locked layer are not in the selection and are not changed. |

Undo brings an erased or changed object back with the properties it had.

Tests: `cad-commands/src/tests/properties.rs`, one per row and command; `cad-document/src/drawing.rs`.

## Layers that are off or locked

| The object's layer is… | Drawn | Snapped to; a cutting edge or boundary | Selected; picked by a command to act on |
| --- | --- | --- | --- |
| On and unlocked | yes | yes | yes |
| **Locked** | yes | yes | **no** |
| **Off** | no | no | no |

- An object on a layer that is off is not there for the pointer at all: a click passes through it, a box does not catch it, TRIM with every object as an edge does not count it.
- An object on a locked layer cannot be changed by any command. A click or a box that would have selected it says how many were left out — "1 was on a locked layer." — and a command that picks the object it acts on (OFFSET, TRIM, EXTEND, FILLET, CHAMFER, BREAK) refuses the pick and names the layer: `That object is on the locked layer "walls".` STRETCH leaves locked objects out of its window and says how many.
- A selected object whose layer is then switched off or locked leaves the selection.
- New objects are put on the current layer, which always exists and is on; it may be locked.

Tests: `cad-commands/src/tests/layers.rs`, `cad-document/src/drawing.rs`, `cad-render/tests/render_boundary.rs`.

## At a glance

| Command | Line | Arc | Circle | Open polyline | Closed polyline | Text |
| --- | --- | --- | --- | --- | --- | --- |
| [MOVE, COPY, ROTATE, SCALE, MIRROR](#move-copy-rotate-scale-mirror) | yes | yes | yes | yes | yes | yes; a mirrored text stays readable |
| [Grip editing](#grip-editing) | ends, midpoint | ends, midpoint, centre | centre, quadrants | vertices | vertices | insertion point |
| [OFFSET](#offset) | yes | yes | yes | yes | yes | no: not a curve |
| [TRIM](#trim) | yes | yes | if crossed twice | yes | if crossed twice | no; never a cutting edge |
| [EXTEND](#extend) | yes | yes | no: it has no end | yes | no: it has no end | no; never a boundary |
| [FILLET](#fillet) | with a line, arc, or circle | with a line, arc, or circle | with a line, arc, or circle; left whole | between two of its own segments | between two of its own segments | no |
| [CHAMFER](#chamfer) | with a line | no | no | between two of its own segments | between two of its own segments | no |
| [BREAK](#break) | yes | yes | two points; becomes an arc | yes | two points; opens | no |
| [JOIN](#join) | yes | yes | no: it has no end | yes | no: it has no end | no: it has no ends |
| [STRETCH](#stretch) | by its ends | by its ends | whole, by its centre | by its vertices | by its vertices | whole, by its insertion point |
| ERASE | yes | yes | yes | yes | yes | yes |
| TEXTEDIT | — | — | — | — | — | changes what it says |

A text is not a curve: it has no primitives, so nothing intersects it, trims against it, or snaps to the strokes of its glyphs — only to its insertion point (the Insertion snap). It is picked and selected by its box, so a click inside a letter's counter picks it. Commands that work on curves refuse it by name: "A text cannot be offset."

Polylines may have arc segments throughout: every command reads a polyline through the same primitives — segments and arcs — as it reads separate lines and arcs.

Every operation above has an invariant held over generated, nearly degenerate input, and the geometry underneath has a regression corpus: see [`test-data/geometry/README.md`](../test-data/geometry/README.md).

## MOVE, COPY, ROTATE, SCALE, MIRROR

These act on a selection, made before the command or at its prompt, and apply one transform to every object in it.

| Command | Transform | Every kind of object |
| --- | --- | --- |
| MOVE | Translation from a base point to a second point | Moved as it is |
| COPY | The same, leaving the originals; repeats until Enter | Copies are new objects |
| ROTATE | Rotation about a base point by a typed angle, or towards the pointer | Arcs keep their sweep; polyline bulges are unchanged |
| SCALE | Uniform scaling about a base point by a typed factor, or by the pointer's distance | Radii scale with it; a factor of zero or less is refused |
| MIRROR | Reflection across a line through two points; asks whether to erase the originals | Arcs are re-oriented so they stay counter-clockwise; polyline bulges change sign; a text takes the mirrored box and reads the right way up in it, as a draughtsman would redraw it |

Rules:

- Quarter turns, mirrors across horizontal and vertical lines, and translations are computed directly, so whole-number coordinates stay whole numbers.
- While the target is being chosen the result is previewed without copying the objects, and the objects being moved are not snap targets (DESIGN §18.2).
- The two points of a mirror line must differ; a rotation or scale needs a base point.

Tests: `cad-geometry/src/transform.rs` (property tests: rotating there and back and mirroring twice are the identity; rotation keeps distances; scaling multiplies them), `cad-document/src/entity.rs`, `cad-commands/src/tests/transform.rs`, `web/e2e/transform.spec.ts`.

## Grip editing

Selecting objects shows their grips; clicking one starts a stretch of that point, which runs as a command — typed coordinates, object snap, ORTHO, POLAR, and undo all apply ([ADR-0010](adr/0010-grip-editing-and-arc-grip-semantics.md)).

| Object | Grip | Dragging it |
| --- | --- | --- |
| Line | An end | Moves that end; the other stays |
| Line | The midpoint | Moves the whole line |
| Circle | The centre | Moves the circle |
| Circle | A quadrant point | Changes the radius |
| Arc | An end or the midpoint | Re-fits the arc through the dragged point and the other two |
| Arc | The centre | Moves the arc |
| Polyline | A vertex | Moves that vertex; arc segments on either side keep their bulge |
| Text | The insertion point | Moves the text |

Refused, leaving the object as it was: a line dragged to no length, a circle to no radius, an arc's three points into a line, a vertex onto its neighbour. Grips are shown for up to 100 selected objects.

Tests: `cad-document/src/grips.rs`, `cad-commands/src/tests/grips.rs`, `web/e2e/grips.spec.ts`.

## OFFSET

| Object | Result | Refused when |
| --- | --- | --- |
| Line | A parallel line of the same length, on the side of the picked point | The picked point is on the line (it chooses no side) |
| Circle | A concentric circle: larger if the point is outside, smaller if inside | The point is on the circle; or an inward offset would leave nothing |
| Arc | A concentric arc with the same angles: larger if the point is outside the arc's circle, smaller if inside | The point is on the arc's circle; or an inward offset would leave nothing |
| Polyline, open or closed, with straight and arc segments | Its offset on the side of the picked point: one polyline, or several where the shape falls apart | The point is on the polyline; or nothing is left of it at that distance |

The distance must be greater than zero and is remembered for the session.

With the `Through` option there is no distance: after the object is picked, the copy is placed so that it passes through the next point. For a line the distance is measured to the line carried on past its ends, so the point may lie beyond them.

Polylines ([ADR-0011](adr/0011-polyline-offset.md)):

- The result is the true offset: every point of it is the offset distance from the polyline. On the inside of a corner the result is sharp; **on the outside of a corner it is an arc**, not a mitre.
- Segments too short to survive disappear; arcs offset past their centre collapse to a corner; a shape with a narrow part comes back as several polylines, all created by the one pick.
- Where a narrow part vanishes, the offset can dip slightly towards it: there the nearest part of the polyline is a corner, and the offset follows an arc about it.
- An open polyline is offset to one side only, with flat ends.

Not implemented: mitred outside corners.

Tests: `cad-geometry/src/offset.rs`, `cad-geometry/src/path_offset.rs` (including the distance property on random polylines), the corpus `test-data/geometry/polyline-offsets.txt`, `cad-document/src/editing.rs`, `cad-commands/src/tests/offset.rs`, `web/e2e/offset.spec.ts`.

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

## FILLET

Two picks; the two objects are cut back or carried on to meet a tangent arc of the set radius.

| First ↓ / Second → | Line | Arc | Circle | Polyline |
| --- | --- | --- | --- | --- |
| Line | yes | yes | yes | no |
| Arc | yes | yes | yes | no |
| Circle | yes | yes | yes | no |
| Polyline | no | no | no | the same polyline: yes |

Rules:

- The objects need not touch. A line is cut back or carried on to the point where the fillet touches it; an arc is cut back or carried round its circle; **a circle is left whole**.
- Of each object, the side that was picked is the side that is kept.
- There can be up to eight arcs of a given radius tangent to two objects. The one used touches each object nearest to where it was picked — so the picks choose the fillet as well as the sides.
- Between two lines, a radius that would use up a whole line is refused: "The radius is too large for these lines." Parallel lines are refused: "The lines are parallel."
- With a curve involved, when no arc of the radius is tangent to both: "No fillet of that radius fits these objects."
- Radius zero makes a sharp corner and no arc: two lines meet at their corner; a line and an arc, or two arcs, meet where their line or circle cross, nearest the picks.
- Both objects keep their identity; the arc is a new object. One fillet is one transaction.
- **Within one polyline:** pick two of its segments that meet at a corner, and that corner is rounded; the arc becomes part of the polyline, which stays one object. Both segments must be straight. Picks on segments that do not share a corner: "Pick two segments that meet at a corner of the polyline."
- **The `Polyline` option** rounds every corner of one polyline in one go and reports how many were done and how many skipped — because a segment was too short for the radius, or an arc. With nothing done, nothing changes.
- A polyline is joined only to itself, not to another object. Picking the same line, arc, or circle twice is refused.
- The radius cannot be negative and is remembered for the session.

Not implemented: fillets between a polyline and another object, or at a polyline corner next to an arc segment; the `Trim` and `Multiple` options.

Tests: `cad-geometry/src/corner.rs` and `cad-geometry/src/curve_fillet.rs` (each with a property test that the fillet arc is tangent to both objects), `cad-geometry/src/path_corner.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/corner.rs`, `web/e2e/fillet-chamfer.spec.ts`.

## CHAMFER

Two picks; two lines are cut back or carried on to a corner and joined by a straight bevel.

| First ↓ / Second → | Line | Arc | Circle | Polyline |
| --- | --- | --- | --- | --- |
| Line | yes | no | no | no |
| Arc, Circle | no | no | no | no |
| Polyline | no | no | no | the same polyline: yes |

Rules:

- The first line picked is cut back by the first distance and the second by the second, measured from where the lines meet; the two points are joined with a line. Both distances zero makes a sharp corner and no line.
- The lines need not touch, and where they cross, the side of each that was picked is kept.
- Both lines keep their identity and direction; the bevel is a new object. One chamfer is one transaction.
- Parallel lines are refused: "The lines are parallel." Distances that would use up a whole line: "The distances are too large for these lines."
- A chamfer is a cut across a corner of two straight lines; an arc or a circle has no such corner. They are refused by name: "CHAMFER joins two straight lines, not arcs."
- **Within one polyline:** pick two straight segments that meet at a corner; the bevel becomes part of the polyline. The first distance belongs to the segment picked first. **The `Polyline` option** bevels every corner, the first distance going to the segment that arrives at each corner in the polyline's own direction; it reports how many were done and how many skipped.
- Distances cannot be negative and are remembered for the session.

Not implemented: chamfers between a polyline and another object; the `Angle`, `Trim`, and `Multiple` options.

Tests: `cad-geometry/src/corner.rs`, `cad-geometry/src/path_corner.rs`, `cad-document/src/editing.rs`, `cad-commands/src/tests/corner.rs`, `web/e2e/fillet-chamfer.spec.ts`.

## BREAK

One pick and one point; the part of the object between them is removed. The same point twice — typed as `@` — splits the object there and removes nothing.

| Object | Two different points | The same point twice (`@`) |
| --- | --- | --- |
| Line | Two lines; one if a point is at or past an end | Two lines meeting at the point |
| Arc | Two arcs; one if a point is at or past an end | Two arcs meeting at the point |
| Circle | An arc: the part counter-clockwise from the first point to the second is removed | Refused |
| Open polyline | Two polylines; one if a point is at or past an end. Arc segments are cut as arcs | Two polylines meeting at the point |
| Closed polyline | One open polyline: the part from the first point to the second, in the polyline's own direction, is removed | Refused |

Rules:

- The first break point is where the object was picked, unless the `First` option is used to name another.
- A break point need not be on the object: the nearest point of the object is used. A point past an end therefore takes that end off.
- The first piece is the object itself, edited; a second piece is a new object. One break is one transaction.
- Breaking at one point that is an end of the object is refused: "That is an end of the object: there is nothing to break off."
- Two points that span the whole object are refused: "That would remove the whole object; use ERASE for that."
- A circle or a closed polyline has no ends, so one point cannot break it: "A circle has no ends: it needs two different break points."
- After a refusal the command asks for the second point again.

Tests: `cad-geometry/src/trim.rs` (`break_*`), `cad-geometry/src/path.rs` (`Path::broken`), `cad-document/src/editing.rs`, `cad-commands/src/tests/break_.rs`, `cad-input/src/point.rs` (the bare `@`), `web/e2e/break.spec.ts`.

## JOIN

Acts on a selection, made before the command or at its prompt. What happens depends on the whole selection, in this order:

| The selection is… | Result | Refused when |
| --- | --- | --- |
| Lines, all on one line | One line from the first end to the last. **A gap between them is bridged**; overlaps are merged | — |
| Arcs, all on one circle | One arc; or a circle, when together they go all the way round | Some of them neither touch nor overlap another: "These arcs are on one circle but do not touch. The Close option makes the whole circle." |
| Anything else: lines, arcs, and open polylines | Those that meet end to end become polylines, one per chain; a chain that returns to its start is closed. Objects that meet nothing are left as they are | Nothing meets anything: "Nothing to join: none of these objects meet end to end." |
| With the `Close` option: arcs of one circle, one or more | The whole circle, gaps or not | Anything else is selected: "Close makes a circle out of arcs of one circle; select only those." |

Rules:

- Two ends are the same point when they are within the linear tolerance (10⁻⁹) of each other — the tolerance class, not a number of its own.
- A gap between collinear lines is bridged because there is only one way to do it. A gap between arcs is not: it could be closed either way round, and the command does not guess.
- Lines that are not *all* collinear are chained instead: two collinear lines and a third at an angle become one polyline with a vertex where the two met.
- Arc segments keep their shape whichever way the chain runs through them. A chain runs in the direction of its oldest member.
- Where more than two ends meet at one point, the two oldest objects are joined there.
- Circles and closed polylines have no ends and are never joined.
- Each result takes the place of its oldest member and keeps that object's identity; the other members are removed. One JOIN is one transaction, however many chains it makes.
- Fewer than two objects: "Select at least two objects to join." A refusal changes nothing.
- JOIN undoes a BREAK at one point exactly for lines and arcs; a polyline keeps the vertex it was broken at.

Not implemented: bridging a gap between arcs; joining to a chosen source object; a fuzz distance for ends that nearly meet.

Tests: `cad-geometry/src/join.rs` (with a property test that a path cut into its segments, shuffled, and turned round chains back into one), `cad-document/src/joining.rs`, `cad-commands/src/tests/join.rs`, `web/e2e/join.spec.ts`.

## STRETCH

Two corners of a crossing window, a base point, and a second point. Every object the window crosses is looked at; the points that define it and lie inside the window move by the displacement from the base point to the second, and the rest stay.

| Object | Its defining points | Some inside | All inside |
| --- | --- | --- | --- |
| Line | Its two ends | The end inside moves; the line changes length and direction | Moves as it is |
| Arc | Its two ends | Re-fitted through the moved end, its old midpoint, and its other end — the rule of its end grips ([ADR-0010](adr/0010-grip-editing-and-arc-grip-semantics.md)) | Moves as it is, even if its bulge reaches outside the window |
| Circle | Its centre | — | Moves as it is. **A circle moves only if its centre is inside**; it never changes size |
| Polyline, open or closed | Its vertices | The vertices inside move. Arc segments keep their bulge, so each is re-fitted between its new ends with the same included angle | Moves as it is |

Rules:

- The window is asked for by the command, corner by corner, and is a crossing window whichever way it is drawn. Its corners are places on the screen: object snap does not apply to them. It applies to the base point and the second point as usual, as do ORTHO, POLAR, and a typed distance.
- An object the window crosses without holding any of its defining points — a line running through it, a circle whose centre is outside — is left as it is. A circle whose centre is inside a window that does not touch the circle itself is not crossed, and is left too.
- With nothing held: "Nothing in the window to stretch.", and the window is asked for again.
- While the second point is being chosen the result is previewed; the objects being stretched are not snap targets meanwhile (DESIGN §18.2).
- An object that would stop being valid — a line stretched to no length, an arc whose three points fall in a line, a polyline vertex moved onto its neighbour — is left unchanged and counted: "1 object could not be stretched that way and was left unchanged." The others are still stretched.
- Every object keeps its identity. One stretch is one transaction.

Not implemented: choosing objects by clicking them (they would move whole) or with several windows; the `Displacement` option; a selection made before the command, which is ignored.

Tests: `cad-document/src/editing.rs` (`stretch_hold`, `stretched`), `cad-commands/src/tests/stretch.rs`, `web/e2e/stretch.spec.ts`.
