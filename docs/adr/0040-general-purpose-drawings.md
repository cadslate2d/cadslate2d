# ADR-0040: General-purpose drawings are 2D views generated from descriptions; nothing 3D is kept

**Status:** Proposed (2026-10-27), for the maintainer to accept at the M24 exit; the milestone's issues are its prototype.  
**Date:** 2026-10-27  
**Kind:** Product boundary and architecture (new generators; no new crate)  
**Design refs:** §3, §4, §45; ADR-0013, ADR-0037, ADR-0038, ADR-0039

## Context

The maintainer (2026-10-26, #330): the program, and the AI with it, should be general purpose. That means façades, cross sections, and isometrics for architecture and mechanical engineering, not only plans and parts.

DESIGN §4 locks out 3D modelling, solid and surface modelling, and BIM; §45 adds 3D authoring. ADR-0037 restates it for the AI. Every one of these drawings is 2D on paper. The question is what each is drawn *from*:
- by hand, line by line, the views of one object drift apart, and the AI drawing them line by line does worse (#328: a Warren truss needed a worked example);
- from a 3D model, they agree, but the model is the suite §45 warns about.

M21 and M22 found a third way: a generator. A description in — a plate's sizes, a floor plan's brief — and ordinary geometry out, deterministic and testable, by command first and through the AI second, with the description kept in the group the drawing makes (#325) so it can be drawn again revised.

## Decision

### 1. A view is drawn by a generator from a description

Each kind of drawing is a generator:
- a **description** in — a small, typed record of the few numbers the kind needs;
- ordinary **2D entities** out, on named layers, in one transaction, one undo.

What is drawn goes in a group whose description holds the description, as a plan's `PLAN-n` does (#325). Editing what is drawn is editing lines: the drawing does not follow the description afterwards. A request to change it draws it again from the description revised.

### 2. The line against a 3D model

A description is not a model. It is never shown, turned, or rendered, and the drawing keeps no 3D object: the entities are 2D, and the description is text in a group. Each generator works out its own kind's views in closed form — the silhouettes of prisms and cylinders, rectangles in a wall — not by a general hidden-surface or solid algorithm.

**What would cross the line:** a description that needs a feature beyond its kind's closed form — fillets between faces, holes at an angle, booleans of solids. That is a solid modeller, and needs a new ADR that weighs it against §4, not one more field.

### 3. The kinds

| Kind | Drawn from | Command | Issue |
| --- | --- | --- | --- |
| Isometric drafting | Nothing: a drafting aid. The isometric snap style, `ISOPLANE`, isocircles, and text on the planes | `SNAP` Style, `ISOPLANE`, `ELLIPSE` Isocircle | #336 |
| Elevations (façades) | A plan drawn by `FLOORPLAN`: its brief, laid out again deterministically (ADR-0039), with the brief's heights — storey, sills and heads, roof | `ELEVATION` | #337 |
| Building sections | The same, and a cut line across the plan; the stair in section where the cut crosses it (#331) | `BSECTION` | #338 |
| Mechanical views | A part: a profile in the front plane — lines and arcs — extruded to a depth with holes through it; or a turned part, a stepped profile of diameters along an axis with a bore | `PARTVIEWS` | #339 |
| Sheets | A paper size, a scale, and the title block's fields | `TITLEBLOCK` | #340 |

**Isometric.** Points on the isometric axes are mapped to the drawing by one affine map: `x` along 30°, `y` along 150°, `z` up, each at true length (an isometric *drawing*, not a projection). A circle on a plane maps to an ellipse, worked out from the plane's two axes.

**Elevations and sections** read the walls, doors, and windows that `cad_layout` drafts (ADR-0039). They add to the brief only heights:
- `storey_height`, already there for stairs;
- the sill and head of windows, the head of doors;
- the roof — flat with a parapet, or pitched (gable or hip) at an angle, with eaves.

Their geometry is pure functions in `cad-layout`, beside the plan's drafting. The stair's section is `cad_geometry::stair`'s, placed by `cad-commands`.

**Mechanical views** are pure geometry in `cad-geometry` (`views`). They draw:
- the front, top, and right views, aligned for first- or third-angle projection;
- visible edges continuous;
- hidden edges with the HIDDEN linetype;
- centre lines with CENTER;
- optionally a full section on a plane through the part, hatched ANSI31, with its cutting-plane line.

For a prism and a turned part with coaxial or perpendicular holes, every edge, silhouette, and hidden edge is a line, an arc, or a circle known in closed form.

### 4. Through the AI

The intent IR grows one operation per kind, each compiled to its command's prompts like every operation (ADR-0037):
- `iso_line`, `iso_polyline`, `iso_circle`;
- `elevation`;
- `building_section`;
- `part_views`;
- `title_block`.

The corpus gains a level, **L4: general-purpose drawings**, judged as L1 and L2 are, by the drawing made. Each kind adds cases in English and Portuguese.

## Alternatives considered

- **A 3D model and projection.** Every view agrees by construction. Rejected: it is the 3D modelling §4 locks out, and the suite §45 warns against.
- **The AI drawing each view line by line.** Nothing new in the program. Rejected: views drift apart, the trusses of #328 showed how the model errs at it, and nothing ties the views together for a revision.
- **Views as blocks or as objects of a new kind.** Rejected for the reason ADR-0013 and DESIGN §4 give for walls: plain geometry on layers is what a drafter edits and what DXF carries.

## Consequences

- No new crate. Elevations and sections extend `cad-layout`'s drafting; mechanical views and isocircles are `cad-geometry`; the commands are `cad-commands`'.
- The brief of a floor plan gains heights. Absent ones take defaults, so a brief written before reads the same.
- Each generator is measured as the floor plan was: unit tests of its geometry, headless tests of its command, and corpus cases through the AI.
- The views of a part agree only as drawn; a change made to one by hand is not carried to the others. That is 2D drafting, and the reason for drawing again from the description.

## Revisit when

- A kind's description needs a feature without a closed-form view (§2).
- Users ask to keep views in step after hand edits; that is associativity, a different product.
