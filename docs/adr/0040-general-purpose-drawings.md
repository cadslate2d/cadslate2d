# ADR-0040: General-purpose drawings are 2D views generated from descriptions; no general-purpose 3D model is kept

**Status:** Accepted (2026-10-27, by the maintainer, who answered the revision with "proceed"). Proposed the same day, and revised after the maintainer's review, which approved the direction and asked for four changes:
- the line against a 3D model stated precisely (§2);
- the isometric convention checked (§3);
- the features each generator supports stated, and the rest refused (§4);
- a plan's views drawn from the plan as drawn, not from its brief laid out again (§3).

The milestone's issues are its prototype.  
**Date:** 2026-10-27  
**Kind:** Product boundary and architecture (new generators; no new crate)  
**Design refs:** §3, §4, §45; ADR-0013, ADR-0037, ADR-0038, ADR-0039

## Context

The maintainer (2026-10-26, #330): the program, and the AI with it, should be general purpose. That means façades, cross sections, and isometrics for architecture and mechanical engineering, not only plans and parts.

DESIGN §4 locks out 3D modelling, solid and surface modelling, and BIM; §45 adds 3D authoring. ADR-0037 restates it for the AI. Every one of these drawings is 2D on paper. The question is what each is drawn *from*:
- **By hand, line by line,** the views of one object drift apart. The AI drawing them line by line does worse (#328: a Warren truss needed a worked example).
- **From a 3D model,** they agree, but the model is the suite §45 warns about.

M21 and M22 found a third way, the generator: a description goes in and ordinary geometry comes out. It is deterministic and testable, used by command first and through the AI second. The description is kept in the group the drawing makes (#325), so the drawing can be made again revised.

## Decision

### 1. A view is drawn by a generator from a description

Each kind of drawing is a generator:
- a **description** in — a small, typed record of the few numbers the kind needs;
- ordinary **2D entities** out, on named layers, in one transaction, one undo.

What is drawn goes in a group whose description holds the description, as a plan's `PLAN-n` does (#325). Editing what is drawn is editing lines: the drawing does not follow the description afterwards. A request to change it draws it again from the description revised.

**A generator either draws the description rightly or refuses it, saying why.** It never draws something that only looks plausible. This is what makes a generator's output worth more than a drawing the AI improvises, and what made the floor-plan solver's refusals meaningful (ADR-0039). Each generator states what it supports (§4), checks each description against it, and refuses the rest with a reason the user — or the AI, explaining it — can act on.

### 2. The line against a 3D model

A part described by a profile, a depth, and holes *is* a three-dimensional object, mathematically, and its generator reasons about it as one to find what each view sees. The line is not drawn at 3D geometry; it is drawn at what the program keeps and maintains:

> **Generators may use implicit three-dimensional geometry to derive views, but the application maintains no general-purpose 3D scene, solid topology, or persistent geometric dependency graph.**

What is kept is the description, as text in a group, and the 2D entities drawn from it. Nothing is shown, turned, or rendered in 3D. Each generator works its views out in closed form for the features it supports, not by a general hidden-surface or solid-modelling algorithm.

**What would cross the line:** a description whose views need a general algorithm — booleans of arbitrary solids, fillets between faces, surfaces meeting along curves the generator cannot state. That is a solid modeller. It needs a new ADR that weighs it against §4, not one more field.

### 3. The kinds

| Kind | Drawn from | Command | Issue |
| --- | --- | --- | --- |
| Isometric drafting | Nothing: a drafting aid. The isometric snap style, `ISOPLANE`, isocircles, and text on the planes | `SNAP` Style, `ISOPLANE`, `ELLIPSE` Isocircle | #336 |
| Elevations (façades) | A plan drawn by `FLOORPLAN` — the plan as drawn, kept with it (below) — and its brief's heights: storey, sills and heads, roof | `ELEVATION` | #337 |
| Building sections | The same, and a cut line across the plan; the stair in section where the cut runs up its first flight (#331) | `BSECTION` | #338 |
| Mechanical views | A part: a profile in the front plane of straight edges, extruded to a depth, with holes through it; or a turned part, a stepped profile of diameters along an axis with a bore | `PARTVIEWS` | #339 |
| Sheets | A paper size, a scale, and the title block's fields | `TITLEBLOCK` | #340 |

**Isometric.** Points on the isometric axes are mapped to the drawing by one linear map: `x` along 30°, `y` along 150°, `z` up, each at true length (an isometric *drawing*, √(3/2) times the projection).

The review asked whether these axes, which are not all 120° apart, are wrong. They are not. The map's two rows — where x, y, and z go across and up the page — are square to each other and equally long. That is exactly an orthographic projection with every axis foreshortened alike. The eye looks along (−1, −1, 1), from above the corner nearest it, which is the lowest corner on the page. The positive axes leave that corner at 30°, 150°, and 90°. The rays along x, along y, and *down* z are the three 120° apart.

Drawing the axes at 330°, 210°, and 90° is the same projection seen from the opposite octant, (1, 1, 1), with the nearest corner the highest. The convention here is drafting's usual one, a box standing on its nearest corner. `cad_core::iso`'s tests prove both properties. A circle on a plane maps to an ellipse worked out from the plane's two axes.

**A plan's views are drawn from the plan as drawn.** A deterministic search is deterministic only relative to its implementation, and ADR-0039 promises it only within one build on one platform. It does differ: the WASM app and the native tests lay out the same brief differently. Laid out again, a brief could give another plan than the one the user has.

So `FLOORPLAN` keeps the plan as drawn in its group's description, after the brief: its layout, its doors and windows as drafted, and where its stairs were drawn. The snapshot is versioned (`cad_layout::Resolved`, `SNAPSHOT_VERSION`). `ELEVATION` and `BSECTION` read the snapshot, never the brief laid out again. A plan whose group keeps no snapshot, or a snapshot of another version, is refused with the way out: draw it again with `FLOORPLAN`.

The AI is sent the brief and where the plan is, not the snapshot. Elevations and sections are of the plan as `FLOORPLAN` drew it, not of later hand edits to its lines. Following those edits would be associativity (§1).

Elevation and section geometry are pure functions in `cad-layout`, beside the plan's drafting. The stair's section is `cad_geometry::stair`'s, placed by `cad-commands`.

**Mechanical views** are pure geometry in `cad-geometry` (`views`):
- the front, top, and right views, aligned for first- or third-angle projection;
- visible edges continuous, hidden edges with the HIDDEN linetype, centre lines with CENTER;
- optionally a full section through the part, hatched ANSI31, with its cutting-plane line.

### 4. What each generator supports, and what it refuses

The review pointed out that a transverse hole through a cylinder meets it along a curve whose views are neither lines nor circles. The same is true wherever a hole enters a sloped face or two holes meet. The generators support only what their closed forms draw rightly, and refuse the rest:

| Generator | Draws | Refuses, with the reason |
| --- | --- | --- |
| `PARTVIEWS`, a prism | A simple profile of straight edges; holes right through along x, y, or z, each wholly inside the part, entering and leaving through faces square to it, so every rim is a circle | A profile that crosses itself. A hole whose centre is outside the part, or that breaks out through a side. A hole that enters or leaves through a sloped face or across a step, where its rim is an ellipse or a broken curve. Two holes that meet |
| `PARTVIEWS`, a turned part | Steps of diameter and length along one axis, a coaxial bore narrower than every step | A bore as wide as a step. Transverse holes, grooves, and threads are not in its description at all |
| `ELEVATION` | One storey under a flat, gable, or hip roof; the outside doors and windows of that side | A plan that keeps no snapshot of itself (§3) |
| `BSECTION` | A cut square to the walls: every wall it crosses, broken at doors and windows; the slabs; the roof's slopes; the openings of the far walls; the stair, where the cut runs up its first flight | A cut that misses the building. A cut running inside a wall, along it. A cut that meets the stair anywhere but up its first flight, where its section is not drawn |

Each refusal is tested at its edge: a case just inside is drawn, one just past is refused. A section draws what it cuts and the far walls' openings, not the interior walls beyond. That is a stated simplification of what a section shows, not geometry drawn wrong.

### 5. Through the AI

The intent IR grows one operation per kind, each compiled to its command's prompts like every operation (ADR-0037):
- `iso_line`, `iso_polyline`, `iso_circle`;
- `elevation`, `building_section`;
- `part_views`, `title_block`.

A description a generator refuses comes back to the model as the command's reason, which it explains to the user.

The corpus gains a level, **L4: general-purpose drawings**, judged as L1 and L2 are, by the drawing made. Each kind adds cases in English and Portuguese.

## Alternatives considered

- **A 3D model and projection.** Every view agrees by construction. Rejected: it is the 3D modelling §4 locks out, and the suite §45 warns against.
- **The AI drawing each view line by line.** Nothing new in the program. Rejected: views drift apart, the trusses of #328 showed how the model errs at it, and nothing ties the views together for a revision.
- **Views as blocks or as objects of a new kind.** Rejected for the reason ADR-0013 and DESIGN §4 give for walls: plain geometry on layers is what a drafter edits and what DXF carries.
- **A plan's views from its brief laid out again.** The first version of this ADR. Rejected after review (§3): the layout depends on the search's version and the platform, so the views could be of another plan than the one drawn.
- **A general 2D composition engine.** Reusable primitives, transformations, constraints, projections, and view procedures, all in 2D, from which drawings no single generator covers could be composed: a pump assembly's section, a gearbox exploded in isometric. It needs no solid modeller, and it could widen what the program draws far more than one generator at a time. It is not taken now: the generators here each answer a request the maintainer made, and a composition engine is a product decision of its own. It is the likeliest answer when requests outside the generators come (below).

## Consequences

- No new crate. Elevations and sections extend `cad-layout`'s drafting, with the plan's snapshot (`Resolved`). Mechanical views and isocircles are `cad-geometry`'s; the commands are `cad-commands`'.
- The brief of a floor plan gains heights. Absent ones take defaults, so a brief written before reads the same.
- A plan's group keeps its snapshot, so its description is longer. The AI is not sent the snapshot.
- Each generator is measured as the floor plan was: unit tests of its geometry and of each refusal at its edge, headless tests of its command, and corpus cases through the AI.
- The views of a part agree only as drawn. A change made to one by hand is not carried to the others. That is 2D drafting, and the reason for drawing again from the description.

## Revisit when

- A kind's description needs a feature without a closed-form view (§2, §4).
- Requests outside the generators recur — an assembly, an exploded view. Then weigh a 2D composition engine (Alternatives) against one more generator, or against refusing them.
- Users ask for views to stay in step after hand edits. That is associativity, a different product.
