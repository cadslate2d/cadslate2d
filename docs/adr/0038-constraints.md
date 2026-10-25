# ADR-0038: Constraints are a table of the drawing, solved by a numeric solver of our own in `cad-geometry`, settled inside the transaction that changes them or their geometry

**Status:** Proposed (2026-10-25; revised the same day after the maintainer's review)  
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
- **Behaviour:**
  - Every edit keeps the constraints.
  - A constraint that would over-constrain the geometry is refused when it is added.
  - A new geometric constraint between two objects moves the second to the first.
- **Storage:** AutoCAD keeps all of this as an associative network of objects (`ACAD_ASSOCNETWORK`). It is undocumented in practice.

The architecture already fixes most of the frame:

- tables change in transactions, with one history (AGENTS.md);
- geometry maths are pure functions in `cad-geometry`;
- tolerances come in classes (§21, ADR-0012);
- properties live beside geometry (ADR-0013);
- what depends on a table is read through it (ADR-0017, ADR-0020).

## The prototype

The solver itself is written: `crates/cad-geometry/src/constraint.rs`, about 700 lines with no dependency beyond `cad-core`, and twelve unit tests. Its measurements are in `benches/benches/constraints.rs`, which builds its drawings in `benches/src/constraints.rs`.

**The test drawings** are plates of four separate lines. Each plate carries eleven constraints:

- four coincident corners;
- two sides horizontal and two vertical;
- a width and a height;
- its lower-left corner coincident with the previous plate's lower right, or fixed for the first.

That makes one long chain, which is one component for the solver: the worst case. Every plate is fully constrained, so the chain has no degree of freedom.

**Every scenario's outcome is asserted before it is timed,** so a fast failure is never reported as a fast solve:

1. **Long-range propagation (solved):** the first plate's width becomes 150, and every plate after it moves 30 to the right.
2. **Local deformation (solved):** a grip holds a corner of the last plate 5 higher. That plate's height dimension is dropped for this scenario, so the drag can be met: its top rises to 85, and every other plate's every coordinate is unchanged.
3. **An over-constrained drag (refused):** the same drag with the height kept, which cannot be met. The solver reports it, and every value comes back as it was.
4. **Separate parts:** the same number of constraints as unchained plates, one of them widened.
5. **Assessing a constraint:** a new parallel between the last plate's opposite sides, which is redundant.
6. **Degrees of freedom:** counted for the chain; 0, as built.

Times are native, release, on the development machine (2026-10-25):

| Constraints (unknowns) | 1. Long-range, solved | 2. Local drag, solved | 3. Over-constrained drag, refused | 4. Separate parts, one changed | 5. Assess a constraint | 6. Degrees of freedom |
| --- | --- | --- | --- | --- | --- | --- |
| 11 (16) | 2.8 µs | 1.3 µs | 10 µs | 2.6 µs | 16 µs | 7.6 µs |
| 99 (144) | 84 µs | 6.0 µs | 269 µs | 9.2 µs | 308 µs | 179 µs |
| 1,001 (1,456) | 6.6 ms | 44 µs | 20 ms | 121 µs | 19.6 ms | 15.1 ms |
| 10,010 (14,560), a probe | 1.36 s | 0.83 ms | — | — | — | — |

What the measurements show:

- **Steps:** the solver takes 2 to 4 Gauss–Newton steps up to 1,000 constraints, and 7 at 10,000.
- **Long-range edits:** an edit that moves geometry along the whole chain costs about quadratic time in the component's size. The conjugate-gradient iterations grow with the chain's length. So 1,000 constraints in *one* component settle in 7 ms, within a frame, and 10,000 do not.
- **Local edits:** a drag that one plate can absorb costs little, however long the chain.
- **A refusal costs more than a solve:** 20 ms at 1,000. The solver tries until its damping gives up, which is at a gesture's end, not per frame. A preview that cannot be met shows the last state that could.
- **Separate parts:** cost is linear in their number, and only the parts an edit touches are solved.
- **Assessing and counting:** both include settling the generic configuration described in §4. That is about 20 ms at 1,000, once per constraint added.

A drawing's constraints are, in practice, many small parts. The 10,000-constraint chain is a probe, not a target.

**The review of this ADR found three faults in the first prototype, all fixed before these measurements:**

- **A half-applied failure:** a held unknown can part one constraint graph into components. One settling while another could not left the geometry half-moved. Now a failure returns every value as it was (`a_held_unknown_that_cannot_be_met_moves_nothing`).
- **Hash-order dependence:** the elimination chose its pivots by iterating a hash map. Ordered maps now.
- **A finite-difference step** of 10⁻⁷ of the drawing's extent lost six digits to rounding. It is now the balanced 6 · 10⁻⁶.

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

`cad_geometry::constraint` is pure functions over a `System`: unknowns, a flag per unknown that holds it, and equations.

**Decomposition.** Union–find joins unknowns that share an equation. Each component is solved alone, and a component that already holds is not touched, so an edit pays only for what it reaches. A held unknown is no longer shared, so holding can part one graph into several components.

**The method** is Levenberg–Marquardt on the weighted residuals `r`, from the current values. An angle's residual is multiplied by the component's extent `L`, so the weight matrix `W` is 1 for a length and `L²` for an angle:

- **Each step** solves the damped normal equations

  > (Jᵀ W J + μ D) Δ = −Jᵀ W r, with D = I and μ = λ · max diag(Jᵀ W J).

  The matrix is symmetric and positive definite, since μ > 0. It is solved by conjugate gradients preconditioned by its diagonal, starting from Δ = 0, until the system's residual is 10⁻¹⁴ of its right-hand side or after 2n iterations. `J Jᵀ` and `JᵀJ` are never formed: the Jacobian is kept as sparse rows.
- **The Jacobian** comes from central differences per equation over its handful of unknowns. The step is 6 · 10⁻⁶ of the larger of the unknown and the component's extent: about the cube root of machine epsilon, balancing truncation against rounding. Analytic derivatives may replace them equation by equation.
- **Acceptance:** a step is taken when it lowers `rᵀ W r`, and λ falls fourfold, to no less than 10⁻¹⁵. Otherwise λ grows eightfold and the step is tried again. λ starts at 10⁻⁶.
- **Success:** every equation holds to its tolerance. A length equation must be within `LinearTolerance` at the magnitude of the coordinates (ADR-0012), and an angle equation within `AngularTolerance`. These are the existing classes, with no new epsilon.
- **Failure:** λ passes 10¹⁰, or 100 steps are spent. Then **every** value of the system comes back as it was, not only the failing component's. Nothing is half-applied. The equations that component left unsatisfied at its least-squares compromise are reported, as the ones to blame.

**What "moves as little as it must" means, precisely:**

- With D = I, every step's displacement lies in the row space of J.
- For constraints linear in the unknowns — coincidence, horizontal, vertical, offsets, fixes — the result is therefore the solution nearest the start: the orthogonal projection onto them (`linear_constraints_settle_at_the_nearest_solution`).
- For non-linear constraints, Levenberg–Marquardt guarantees no global minimum of displacement. It reaches a solution by small least-squares steps from the start, so a solution near it, but not provably the nearest.
- What keeps a grip drag from making unrelated geometry jump is therefore three things together:
  - the components (unrelated geometry is not in the system solved);
  - the held unknowns;
  - the small damped steps from where the geometry is.

  The scenario-2 assertion (every other plate's every coordinate unchanged) is the test of that, and #270 adds a property test of it.

**Assessing a new constraint** — `assess(system, equations)`:

- **The whole constraint is judged at once.** All its equations are judged together, among the components they touch. A coincidence is two equations, and if one of them is already implied, the coincidence over-constrains (`a_constraint_is_judged_by_all_its_equations_together`).
- **Rank is judged at a generic configuration, not where the geometry happens to be.** The free unknowns are moved by small pseudo-random amounts from a fixed seed (10⁻³ of the extent) and settled back onto the existing equations. The rank of the Jacobian there is the constraints' rank. The rank at the current configuration can be lower by accident: two points that happen to meet give a distance between them no gradient, though nothing makes them meet.
- **The rank comes from a sparse Gaussian elimination** of the Jacobian rows, with pivots chosen by size, ties to the lowest column, and entries below 10⁻⁹ of the largest taken as zero.
- **Four outcomes, which the commands keep apart:**

  | Outcome | When | What the command does |
  | --- | --- | --- |
  | Independent | The constraint raises the generic rank by its number of equations. Solving then says whether it can be met: if not, the edit is refused like any other | Adds it |
  | Redundant | It does not raise the generic rank, and already holds | Refuses: it over-constrains the geometry |
  | Conflicting | It does not raise the generic rank, and does not hold: it contradicts the others | Refuses, naming them |
  | Degenerate (alongside the verdict) | The rank at the current configuration is lower than the generic rank | Says so (`a_degenerate_configuration_is_not_taken_for_redundancy`) |

- **Degrees of freedom:** `freedom(system)` is the free unknowns less the generic rank. The constraint bars use it to show what is still free.

**Determinism.** Equal inputs give bit-identical results:

- no result depends on hash-map iteration: components, rows, and pivots are kept in ordered collections;
- the generic configuration's perturbation comes from a fixed seed;
- components are independent, so the order they are found in changes nothing.

The same equations in another order give the same geometry within tolerance, not necessarily the same bits (`solving_is_deterministic`).

### 5. The transaction settles itself

The rule is in `TransactionBuilder::commit`, so every command, every grip (ADR-0010), and every AI operation (ADR-0037) keeps constraints without code of its own.

**The solver's input is the prospective drawing:** the drawing as it would be after the transaction, entities and constraint rows alike.

**What triggers a solve.** The components to solve are those touched by any of:

- an entity the transaction replaces or moves;
- a constraint row it adds;
- a constraint row whose value it changes: `d1` from 120 to 140 changes only a row, and still settles the geometry.

Removing a constraint can break nothing, so it triggers nothing.

**What is held:**

- **Unknowns the transaction set:** of every entity it replaced, the unknowns whose values it changed. A grip's point stays where it was put.
- **The first object of a new geometric constraint:** a command that adds a constraint between two objects asks the builder to hold the first object's unknowns. The second moves to it, as in AutoCAD.
- **Nothing else.** A changed dimension's value holds nothing beyond the above, and the solver takes the solution reached from the current geometry. With the usual fix or a first constrained corner, that is the far side moving; with nothing fixed, both sides move, as little as they can (§4).

**What a successful solve adds:** the settled replacements of the other entities, in the **same transaction**. One edit is one undo, the moved rest included.

**Refusal:** an edit that cannot be settled is not applied. The commit's report says why, in the constraint's terms ("The edit would break d1 = 120"), from the equations to blame, and the session logs it.

**Previews** settle the same way on the session's scratch copy, so a grip drag shows the rest of the part following.

**What counts as a change:**

- Moving every entity of a part together (MOVE, ROTATE) holds everything and breaks nothing but a `Fix`.
- Scaling breaks dimensions, as in AutoCAD.
- An edit that only changes properties settles nothing.

An unconstrained drawing pays one lookup per touched entity.

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
- **Rank at the current configuration only:** cheaper, but it takes a degenerate configuration for a redundant constraint, and refuses a constraint a drafter means.
- **Conjugate gradients on J Jᵀ to test whether a gradient lies in the others' span** (the prototype's first try): it stopped recognising redundancy at about twenty plates, because J Jᵀ's conditioning worsens with the chain. The sparse elimination is exact to its threshold at every size measured.
- **Constraints as properties of entities rather than a table:** a constraint relates several entities. As a row it changes once, is removed once, and is read by name, like a group (#214).
- **Solving outside the transaction:** a command would settle and then commit a second transaction. That gives two undos for one edit, and a command that forgets to settle breaks the drawing. The builder settling at commit makes it impossible to forget.

## Consequences

- `cad-geometry` gains `constraint`, with no new dependency.
- `cad-document` gains:
  - the constraints table and its `Change`;
  - the mapping between entities and the solver;
  - settling in `commit` over the prospective drawing, with a way for a command to hold an object, and a refusal in the report.
- DESIGN §19.2's list of what a transaction changes gains constraints.
- An edit of an unconstrained drawing pays one lookup: whether any constraint refers to a touched entity.
- Large single components, beyond about 3,000 constraints, settle in more than a frame. The probe measured 1.36 s at 10,000.
- The constraint commands, the bars, re-solving on edit, DXF, the generators, and the AI's use follow in #266–#274, in that order.

## Revisit when

- A drawing's single component grows beyond a few thousand constraints in use. The answers then are a better preconditioner (incomplete Cholesky) or rigid-cluster decomposition before the numeric solve.
- Refusals at that size need to be faster than a gesture: stop the damping earlier when the residual stalls.
- A drag in use makes geometry jump that a nearest-solution formulation would not. That calls for minimising the displacement explicitly, as a weighted objective, rather than relying on small steps.
- Drafters ask for expressions between dimensional constraints (`PARAMETERS`).
- Splines or ellipses are to be constrained (smooth, G2).
- A mature, licence-compatible solver crate appears that would replace ours with less code.
