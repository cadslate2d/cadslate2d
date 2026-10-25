# ADR-0038: Constraints are a table of the drawing, solved by a numeric solver of our own in `cad-geometry`, settled inside the transaction that edits their geometry

**Status:** Proposed (2026-10-25)  
**Date:** 2026-10-25  
**Kind:** Evidence (a new subsystem; amends DESIGN §19.2's list of what a transaction changes)  
**Design refs:** §15, §19, §21; ADR-0001, ADR-0012, ADR-0013, ADR-0017, ADR-0020, ADR-0037

## Context

M21 (#264) asks for persistent geometric and dimensional constraints, AutoCAD-style. They have two uses:

- A drafter types `GEOMCONSTRAINT` and `DIMCONSTRAINT`.
- The part generators and the AI's second level ask for "a 120 × 80 plate with four Ø8 holes 10 from the corners" and later "make it 20 wider without moving the holes relative to the edges".

A constraint system touches the document, the history, every edit, and the file. DESIGN says nothing of it, so the decisions are made here, before code (#265).

AutoCAD's model, which drafters know:

- **Geometric constraints:** coincident, collinear, concentric, fix, parallel, perpendicular, horizontal, vertical, tangent, smooth, symmetric, and equal.
- **Dimensional constraints:** linear (horizontal, vertical, aligned), angular, radial, and diameter. They have names (`d1`) and values, and are shown as dimensions that are not plotted.
- **Behaviour:** every edit keeps the constraints. A constraint that would over-constrain the geometry is refused when it is added.
- **Storage:** AutoCAD keeps all of this as an associative network of objects (`ACAD_ASSOCNETWORK`). It is undocumented in practice.

The architecture already fixes most of the frame:

- tables change in transactions, with one history (AGENTS.md);
- geometry maths are pure functions in `cad-geometry`;
- tolerances come in classes (§21, ADR-0012);
- properties live beside geometry (ADR-0013);
- what depends on a table is read through it (ADR-0017, ADR-0020).

## The prototype

The solver itself is written: `crates/cad-geometry/src/constraint.rs`, about 700 lines with no dependency beyond `cad-core`, with its unit tests. Its measurements are in `benches/benches/constraints.rs`, which builds its drawings in `benches/src/constraints.rs`.

The test drawings are plates of four separate lines. Each plate carries eleven constraints:

- four coincident corners;
- two sides horizontal and two vertical;
- a width and a height;
- its lower-left corner coincident with the previous plate's lower right, or fixed for the first.

That makes one long chain, which is one component for the solver: the worst case. Every plate is fully constrained, so the chain has no degree of freedom.

Times are native, release, on the development machine (2026-10-25):

| Constraints (unknowns) | Width of the first plate changed: every plate after it moves | A corner of the last plate dragged by a grip | The same constraints as separate parts, one changed | Is a new constraint redundant? | Degrees of freedom |
| --- | --- | --- | --- | --- | --- |
| 11 (16) | 2.7 µs | 1.4 µs | 2.7 µs | 5.0 µs | 3.1 µs |
| 99 (144) | 81 µs | 6.2 µs | 9.9 µs | 100 µs | 95 µs |
| 1,001 (1,456) | 7.1 ms | 48 µs | 133 µs | 7.0 ms | 7.5 ms |
| 10,010 (14,560), probe | 1.36 s | 0.83 ms | — | — | 0.67 s |

What the measurements show:

- **Steps:** the solver takes 2 to 4 Gauss–Newton steps up to 1,000 constraints, and 7 at 10,000.
- **Long-range edits:** an edit that moves geometry along the whole chain costs about quadratic time in the component's size. The conjugate-gradient iterations grow with the chain's length. So 1,000 constraints in *one* component settle in 7 ms, within a frame, and 10,000 do not.
- **Local edits:** a drag at the end of the chain moves little and costs little.
- **Separate parts:** cost is linear in their number, and only the parts an edit touches are solved.

A drawing's constraints are, in practice, many small parts. The 10,000-constraint chain is a probe, not a target.

## Decision

### 1. What a constraint refers to

A constraint refers to **points and edges of entities**, as `(EntityId, Part)`. `EntityId`s are stable, and never reused within a drawing.

| Part | Of |
| --- | --- |
| `Start`, `End` | a line, an arc |
| `Center` | a circle, an arc |
| `Vertex(n)` | a lightweight polyline |
| `Point` | a point entity |
| `Edge` | a line, a circle, an arc (the whole curve) |
| `Segment(n)` | a straight segment of a lightweight polyline |

Those entity kinds are the ones constrained in this milestone. Any other kind, and a polyline's arc segment as an edge, is refused with a message. Their vertices can still be constrained.

### 2. Where constraints live

Constraints live in **a table of the drawing**, `Table<ConstraintRow>`. It follows AGENTS.md's rule for a table:

- a `Change` variant;
- one arm in `Drawing::apply`;
- builder methods that refuse a row referring to an entity that does not exist or to a part it does not have.

It has one history with every other change, so `U` takes a constraint back as it takes back a line.

A row holds:

- its kind;
- its references;
- for a dimensional constraint, its name (`d1`, `d2`, …, unique in the drawing) and its value in drawing units or radians.

The rules around it:

- **Removing an entity removes its constraints** in the same transaction, as removing one takes it out of its groups (#214).
- **A copy is not constrained:** `insert_like` copies properties, not constraints.
- **Model and layouts only:** constraints relate entities of one space, and none are kept in block definitions in this milestone.

### 3. The kinds, and the equations they are

Each constraint is one or more scalar equations, `cad_geometry::constraint::Equation`, over the unknowns of the points and radii it refers to:

| Constraint | Equations |
| --- | --- |
| Coincident | two: `Equal` of x and of y |
| Horizontal, vertical | one `Equal` of the ends' y, or x |
| Parallel, perpendicular | one: the sine, or the cosine, of the angle between |
| Collinear | `Parallel`, and `PointOnLine` of one end |
| Concentric | two `Equal`s of the centres |
| Equal | `EqualLength`, or `Equal` of the radii |
| Tangent | `LineTangent` (on the side the circle is), or `CircleTangent` (outside or inside, as found) |
| Symmetric | `MidpointOnLine` and `PerpendicularTo` |
| Fix | `Fixed` of each coordinate |
| Horizontal, vertical, aligned distance | `Offset`, or `Distance` |
| Angular | `Angle` |
| Radial, diameter | `Fixed` of the radius, or of the radius as half the diameter |

A point on an object, which AutoCAD's coincident allows, is `PointOnLine` or `PointOnCircle`. Smooth (G2) is left out: it is for splines, which are not constrained here.

**Mapping between entities and the solver.** It is a pair of pure functions in `cad-document`:

- a drawing and a set of constraints give a `System` of unknowns and equations;
- a settled `System` gives back the replaced entities.

An arc is its centre, radius, and two end points on it, so its angles follow its ends. A polyline is its vertices.

### 4. The solver

The solver is `cad_geometry::constraint`, pure functions:

- **Decomposition:** the system splits into independent components, unknowns joined by an equation, with union–find. A component already satisfied is not touched, so an edit pays only for the parts it reaches.
- **Damped Gauss–Newton (Levenberg–Marquardt):**
  - Each step is the least-squares smallest change that reduces the residuals, so geometry moves no more than it must. A grip drag does not make the rest of the part jump.
  - Each step's linear system is solved by Jacobi-preconditioned conjugate gradients over the sparse Jacobian. The Jacobian is never formed densely.
  - An angle's residual is weighed as a length, by the component's extent.
- **Tolerances:** a solution holds when every length equation is within `LinearTolerance` at the magnitude of the coordinates (ADR-0012), and every angle equation within `AngularTolerance`. These are the existing classes, with no new epsilon.
- **Held unknowns:** the unknowns an edit just set are held, and the solver may not move them.
- **Conflicts:** a component that cannot be settled keeps its values, and reports its unsatisfied equations. Nothing is half-applied.
- **Over-constraint and degrees of freedom:** both come from one sparse elimination of the Jacobian.
  - `redundant(system, equation)`: the new equation's gradient is a combination of its component's. Then the constraint is refused as over-constraining: redundant if it already holds, conflicting if not.
  - `freedom(system)`: the free unknowns less the Jacobian's rank. The constraint bars use it to show what is still free.

**Derivatives** are central differences per equation over its handful of unknowns. Analytic derivatives can replace them equation by equation, if precision or speed ever asks.

### 5. Edits keep constraints

**The transaction settles itself.** The rule is in `TransactionBuilder::commit`, so every command, every grip (ADR-0010), and every AI operation (ADR-0037) keeps constraints without code of its own:

1. A transaction that replaces or moves constrained entities holds the unknowns it changed.
2. It solves their components.
3. It adds the settled replacements of the other entities to the **same transaction**. One edit is one undo, the moved rest included.

**Refusal:** an edit that cannot be settled is not applied. The report says why, in the constraint's terms ("The edit would break d1 = 120"), and the session logs it.

**Previews** settle the same way on the session's scratch copy, so a grip drag shows the rest of the part following.

**What counts as a change:**

- Moving every entity of a part together (MOVE, ROTATE) holds everything and breaks nothing but a `Fix`.
- Scaling breaks dimensions, as in AutoCAD.
- An edit that only changes properties settles nothing.

### 6. Values, names, and display

Dimensional constraints are plain values, with names. Expressions between them (`d2 = d1 / 2`, AutoCAD's `PARAMETERS`) are out of this milestone; the names are kept so that they can come later.

Constraints are shown by the overlay:

- constraint bars beside the geometry;
- dynamic dimensions for the dimensional constraints.

They are not in the committed scene and are never plotted. A toggle hides them (#269).

### 7. The file

Constraints round-trip through DXF as **CadSlate XRECORDs** in a named-object dictionary of our own (#271). AutoCAD's associative network, when a file holds one, is reported as not held, never guessed at. A DXF writer that does not know ours keeps its geometry: the constraints are lost and the drawing is not.

## Alternatives considered

- **A solver from crates.io.** A search on 2026-10-25 found five:

  | Crate | Licence | Age | Why not |
  | --- | --- | --- | --- |
  | `slvs` | GPL-3.0 | created 2023, last released 2023-06 | Wraps SolveSpace in C++: a 1.6 MB crate to build for WebAssembly |
  | `brepkit-sketch` | AGPL-3.0-only | created 2026-08 | The licence: ADR-0035 lets the maintainer license the code under other terms as well, which a dependency under *only* AGPL forbids |
  | `arael-sketch-solver` | MIT | created 2026-03 | A framework with its own macros and symbolic differentiation |
  | `sketch-solver` | MIT or Apache-2.0 | one release, 2026-10-07 | Young and unproven |
  | `acs-solver` | MIT | created 2026-10-03 | Young and unproven |

  None models a drawing's entities, the tolerance classes, or held unknowns as the document needs them. The prototype covers what M21 needs in about 700 lines, measured.
- **FreeCAD's planegcs** (C++, LGPL): the same C++ and WebAssembly cost as SolveSpace, for a solver whose method, Gauss–Newton with decomposition, is the one chosen here.
- **A constructive solver** (decomposition into rigid clusters, solved by ruler and compass):
  - For: exact, and fast on well-constrained sketches.
  - Against: much more code; it fails on what it cannot decompose, so a numeric solver is needed anyway.
  - It may come later as a preconditioner of the numeric one.
- **Dense Newton with an LU solve:** simple and fine to a few hundred unknowns, but its cubic cost makes 1,000 constraints in one part take seconds. Conjugate gradients over the sparse Jacobian kept it to 7 ms.
- **Constraints as properties of entities rather than a table:** a constraint relates several entities. As a row it changes once, is removed once, and is read by name, like a group (#214).
- **Solving outside the transaction:** a command would settle and then commit a second transaction. That gives two undos for one edit, and a command that forgets to settle breaks the drawing. The builder settling at commit makes it impossible to forget.

## Consequences

- `cad-geometry` gains `constraint`, with no new dependency.
- `cad-document` gains:
  - the constraints table and its `Change`;
  - the mapping between entities and the solver;
  - settling in `commit`, with a refusal in the report.
- DESIGN §19.2's list of what a transaction changes gains constraints.
- An edit of an unconstrained drawing pays one lookup: whether any constraint refers to a touched entity.
- Large single components, beyond about 3,000 constraints, settle in more than a frame. The probe measured 1.36 s at 10,000.
- The constraint commands, the bars, re-solving on edit, DXF, the generators, and the AI's use follow in #266–#274, in that order.

## Revisit when

- A drawing's single component grows beyond a few thousand constraints in use. The answers then are a better preconditioner (incomplete Cholesky) or rigid-cluster decomposition before the numeric solve.
- Drafters ask for expressions between dimensional constraints (`PARAMETERS`).
- Splines or ellipses are to be constrained (smooth, G2).
- A mature, licence-compatible solver crate appears that would replace ours with less code.
