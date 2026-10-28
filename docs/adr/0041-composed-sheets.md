# ADR-0041: A sheet is composed by operations that place and tie its parts; the model draws inside them

**Status:** Proposed (2026-10-27), for the maintainer's review. Asked for on 2026-10-27 ("proceed"), after the Iron Man and spillway sheets.  
**Date:** 2026-10-27  
**Issue:** #350 · **Epic:** #360, milestone M25  
**Kind:** Architecture and the intent IR (new operations, version 15; no new crate)  
**Design refs:** §4, §45; ADR-0029, ADR-0030, ADR-0037, ADR-0040 (its "general 2D composition engine" alternative)

## Context

ADR-0040 drew general-purpose drawings by generators, one per kind of drawing. It set aside a general 2D composition engine until requests outside the generators recurred. They have:
- **A spillway sheet**: plan, two sections, an elevation, a detail.
- **A powered-armour sheet**: front, side and rear views, helmet details, a section, an exploded assembly, a bill of materials.

Neither is a kind a generator draws. The model drew both by hand, operation by operation, every coordinate its own arithmetic. With intent version 14 and the sheet review (#349), the armour sheet is drawn true size, dimensioned, and aligned. What is still wrong is structural, and the review can only report it, not prevent it:
- **Views drift:** a side view's heights are recomputed, not carried across from the front.
- **Things collide:** labels, notes, and dimensions fall where the next view goes.
- **Sheet machinery is loose text:** section markers, part numbers, and the bill of materials do not refer to each other.

None of this is the model lacking shapes. It is the model doing layout and bookkeeping that a program does better.

## Decision

### 1. The sheet's structure is declared; the program places it

New operations declare what a sheet holds. The engine places and draws it.
- **`sheet`**: paper and scale, as `title_block` has them. It draws the title block and defines the drawing area inside the border.
- **`view`**: a named view, its kind, and its size in the drawing's units:
  - kinds: `front`, `top`, `side`, `rear`, `section`, `detail`, `table`;
  - the size is the extent the model will draw in it;
  - it may be projected from another view (`from: "front"`). A projected view is placed in line with its parent: a side view at the same heights, a top view at the same widths, by the sheet's projection, first or third angle.
- The program packs the views into the drawing area, in projection where declared, with room for titles and dimensions. Each view is drawn with its frame label ("SECTION A-A", "DETAIL B (2:1)"). Placement is a pure function of the declarations, so it is deterministic and tested; it is computed when the document is compiled.

A view that will not fit is refused, with the sizes that would.

### 2. Operations draw inside a view, in its own coordinates

Every drawing operation may name a view: `"in": "front"`. Its coordinates are then local to that view: the origin at the view's lower left, its own units. The compile step moves them to where the view was placed. The model never computes a sheet position.

Geometry is drawn **true size**. A detail view at another scale is drawn scaled by its ratio. Its dimensions measure true size through a **dimension linear factor** (DIMLFAC) on a dimension style made for that view. That is the one addition the dimension style needs.

### 3. What a sheet's parts refer to, they refer to by name

- **`section`** (`view` of kind `section`) names its cutting line in a parent view. The program draws the cutting-plane line, the arrows, and the letters, and titles the section view to match.
- **`parts`** declares the bill of materials: number, name, quantity, material. A **`table`** view draws it as a table.
- **`balloon`** points at a part by number: a leader to a point in a view, the number in a circle. A balloon for a part not in `parts` is refused.
- **`dim_chain`** dimensions a list of points along one side in a chain, its offset from the geometry chosen by the program (DIMCONTINUE, as typed).
- **`notes`**: a block of text placed in a reserved area of the sheet, wrapped to its width.

### 4. Layers and linetypes are declared

`layer` gains `color` and `linetype` (CONTINUOUS, CENTER, HIDDEN, PHANTOM). The linetype is loaded from the program's own table. Centre lines and hidden edges are then a layer away. This is #348's last criterion.

### 5. The review checks the structure too

The sheet review (#349) gains structural checks:
- geometry outside its view's frame;
- views overlapping;
- a section without its marker;
- a balloon without its part.

With the structure declared, most of these become impossible rather than found.

### 6. What it does not do

- **No associativity.** A view is not kept in step with another after an edit (ADR-0040 §1). The views agree because they are placed together and drawn from the same numbers in one document.
- **No projection of geometry.** The program does not compute a side view from a front view. That would be 3D (ADR-0040 §2). It aligns the frames; the model draws each view.
- **No paper space.** Views are regions of model space, not viewports on a layout. Layouts (ADR-0030) remain the drafter's tool for plotting several scales. A later ADR may map a composed sheet onto a layout's viewports.

## Alternatives considered

- **More review, no structure.** Let the model keep doing layout, and catch more of it. Rejected as the main path: each finding costs a revision call, and alignment is not something a review can make the model get right.
- **Views as viewports on a layout** (ADR-0029, ADR-0030). This is the professional way to put several scales on one sheet, and it needs no DIMLFAC. Not now: the AI's proposals, the corpus's judge, and the review all work in model space. A layout would put a sheet where the user, at the model tab, does not look. Noted as the likely next step (§6).
- **A generator per kind of sheet** (a spillway generator, an armour generator). Rejected: there is no end of kinds. That is why ADR-0040 named this alternative.

## Consequences

- Intent version 15: `sheet`, `view`, `parts`, `balloon`, `dim_chain`, `notes`, and the `in` field on drawing operations; `layer` gains `color` and `linetype`.
- New commands, each typed as well as compiled to: `SHEET`, `VIEWFRAME`, `SECTIONMARK`, `BALLOON`, `BOMTABLE`, `NOTES`. Each is registered, has headless tests, and is on a menu.
- The dimension style gains a linear factor, with its DXF group (DIMLFAC, 144).
- The corpus gains a level, **L5: composed sheets**, judged by what the sheet must hold: its views, their alignment, its sections, parts, balloons, and dimensions. In English and Portuguese, with the spillway and the armour among them.

## Revisit when

- A composed sheet needs several scales printed exactly: map it onto a layout's viewports.
- Requests ask for views computed from a solid. That is a solid modeller, and ADR-0040 §2 applies.
