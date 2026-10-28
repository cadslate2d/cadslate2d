# ADR-0041: A sheet is composed by operations that place and tie its parts; the model draws inside them

**Status:** Proposed (2026-10-27). Asked for on 2026-10-27 ("proceed"), after the Iron Man and spillway sheets. Revised the same day after the maintainer's review, which accepted the direction and asked for six changes before acceptance. Their resolution:
- shared geometric references (§3);
- the scale contract (§4);
- placement that knows the annotations (§2);
- projection declared precisely (§1);
- references validated and the sheet kept (§6, §7);
- the boundary stated rightly (§8).

It also asked for L5 to score four things apart (§9).  
**Date:** 2026-10-27  
**Issue:** #350 · **Epic:** #360, milestone M25  
**Kind:** Architecture and the intent IR (new operations, version 15; no new crate)  
**Design refs:** §4, §45; ADR-0020, ADR-0029, ADR-0030, ADR-0037, ADR-0040 (its "general 2D composition engine" alternative)

## Context

ADR-0040 drew general-purpose drawings by generators, one per kind of drawing. It set aside a general 2D composition engine until requests outside the generators recurred. They have:
- **A spillway sheet**: plan, two sections, an elevation, a detail.
- **A powered-armour sheet**: front, side and rear views, helmet details, a section, an exploded assembly, a bill of materials.

Neither is a kind a generator draws. The model drew both by hand, operation by operation, every coordinate its own arithmetic. With intent version 14 and the sheet review (#349), the armour sheet is drawn true size, dimensioned, and roughly aligned. What is still wrong is structural:
- **Views disagree.** A side view's heights are worked out again, not carried across from the front.
- **Things collide.** Labels, notes, and dimensions fall where the next view goes.
- **Sheet machinery is loose text.** Section markers, part numbers, and the bill of materials do not refer to each other.

Most of this is layout, arithmetic, and bookkeeping, which a program does better. One part is not: **aligning the frames of two views does not make the geometry inside them agree.** A front view can put a shoulder at 800 mm and the side view at 820 mm, in frames perfectly in line. This ADR gives that its own mechanism (§3), and says exactly what it does and does not guarantee.

## Decision

### 1. The sheet's structure is declared, and projection precisely

- **`sheet`**: `paper`, `portrait`, `scale` N of 1:N, `projection` (`first` or `third` angle), and the title block's fields. It draws the title block and defines the drawing area inside the border, less the block.
- **`view`**: a unique `id` (letters, digits, `-`), a `title`, a `size` (the extent of its geometry, real units), and its placement, one of the following.
  - **Principal**: the root view, the front, usually. One per sheet at most.
  - **Projected**: `of` a parent view and `looking` `from_left`, `from_right`, `from_above`, `from_below`, or `from_behind`.
    - The slot follows the sheet's projection. In third angle a view from the left goes left of its parent and a view from above goes above. In first angle each goes on the opposite side.
    - A view from behind goes beyond the side view, on the far side of whichever side view its row has.
    - A horizontal neighbour shares its parent's heights; a vertical neighbour shares its widths.
    - A projected view takes its parent's scale; another scale is refused.
  - **Free**: `section`, `detail`, `table`, or `notes` views. They are packed into the space the projected views leave.

  `ratio` (paper : real, such as `2` for 2:1) is a free view's own. Every other view is at the sheet's scale.

Projected views form a tree rooted at the principal view. The following are refused, each saying which:
- a cycle;
- an unknown parent;
- two views in one slot;
- a projected view at a scale other than its parent's;
- a second principal view.

**Position is not geometry.** A view placed from the left of the front is positioned as that view is in the projection. The program does not draw what is seen from the left: the model does, inside it. §3 is what makes the two agree where it matters.

### 2. Views are placed in two passes, knowing their annotations

Declared extents are not rendered extents: dimensions, balloons, labels, and section letters reach past the geometry. Placement is therefore done by the engine, not when the document is compiled.

1. **Measure.** Each view's operations are run on a scratch drawing at the view's own origin. Their rendered bounds are measured: `Entity::bounds` with the drawing's tables, so dimension text and leaders count. The view's title is added.
2. **Place.** The views are placed by a pure function of the sheet, the projection tree, and the measured boxes. Each projected view goes in its slot, aligned on the shared axis, with a gap of 10 paper mm. Free views are packed into the space left, largest first, in rows.
3. **Draw.** The operations run again on the drawing itself, each view's moved to its place. This is one transaction, one undo.

Placement never overlaps two views. If a view cannot be placed, the sheet is refused, saying which view and how much room it lacks. It is never drawn crowded.

The function is deterministic and has property tests: projected neighbours share their axis exactly; no two placed boxes overlap; every box is inside the drawing area.

### 3. Shared references: datums

A **`datum`** names a value of the object being drawn, real units:
- `name`;
- `axis`: `height`, `width`, or `depth`;
- `value`.

For example, the shoulder at height 1450, the reactor's centre at width 0, the chest's front face at depth 170.

Any coordinate of an operation inside a view may be given as `{"datum": "shoulder"}` instead of a number. The view's orientation decides which datums apply to which axis:
- a front view's x is width and y is height;
- a side view's x is depth and y is height;
- a top view's x is width and y is depth.

A datum used on an axis the view does not show is refused. Datums are resolved when the document runs; they are not kept (not associativity, ADR-0040 §1).

**What is guaranteed, and what is not.** A feature drawn at a datum in two views is at the same place in both: one number, used twice. A feature drawn with raw numbers in two views is not checked against itself. Cross-view agreement is guaranteed exactly as far as datums are used. The service's instructions ask the model to declare a datum for every size the request gives and every feature more than one view shows. The review (§7) reports a datum declared and used in only one view of several that show its axis.

There is no constraint solver between views, and none is needed: a datum is a name for a number.

### 4. The scale contract

Units:
- **Real units**: the drawing's unit (`mm`). A view's geometry is given in real units, in its own coordinates.
- **Paper millimetres**: what is printed.
- **Model units**: where entities are stored. The sheet is 1:N, so one paper millimetre is N model units: the sheet's border is `paper size × N`.

A view at ratio r (paper : real) shows a real length L as L·r paper mm, which is L·r·N model units. So the view's **geometry factor is k = r·N**. A view at the sheet's own scale has r = 1/N, so k = 1, and its geometry is stored true size, as #348 asked.

Transformation order, for each point of a view's geometry:
1. resolve datums;
2. scale by k about the view's origin;
3. translate to the view's placed origin.

Nothing is rotated.

What is sized by paper and not by k:
- **Text, dimension text and arrows, balloons, and labels** are given in paper millimetres. They are multiplied by N, so they print the same size in every view, whatever its ratio.
- **Dimensions** in a view use a dimension style made for it: overall scale N, and **linear factor 1/k** (DIMLFAC, DXF group 144). Its numbers read true size: in a 2:1 detail of a 1:3 sheet, k = 6, and a 10 mm feature drawn 60 units long reads 10.
- **Hatch** spacing is set for paper: a view's hatch scale is multiplied by N, not k, so every view's hatch prints alike.
- **Linetypes**: the drawing's LTSCALE is N, so dashes print alike everywhere. A linetype scale per object is not in the program (DESIGN, M5) and is not needed under this rule.

### 5. What a sheet's parts refer to, they refer to by name

- **`section`** is a free view with `cut`: a parent view's `id` and two points in that view's coordinates. The program draws the cutting-plane line, the arrows, and the letters in the parent, and titles the section ("SECTION A-A"). Letters are given in order, skipping I, O, and Q.
- **`parts`** declares the bill of materials: unique positive `number`, `name`, positive integer `quantity`, optional `material`. A **`table`** view draws it.
- **`balloon`**: a `part` number, a view, and a point there. It is drawn with a leader and a circle; balloons are kept clear of each other by the placement of their circles.
- **`dim_chain`**: points along one side of a view, chained, offset from the view's geometry by the program (DIMCONTINUE, as typed).
- **`notes`**: lines of text in a `notes` view, wrapped to its width.

### 6. References are validated; the sheet is kept

The following are refused, each saying which:
- a duplicate view id;
- an unknown view, parent, or part;
- a cycle;
- a section whose cut is not inside its parent's geometry;
- a section letter used twice;
- a balloon for a part not declared;
- a part number that is not unique, or not positive;
- a quantity that is not a positive integer;
- a datum unknown, or on an axis the view does not show.

A composed sheet is kept as ADR-0040 keeps a generator's description: a group `SHEET-n` whose description holds the declaration. That is the sheet, its views with their placed origins, ratios, and geometry factors, its datums, and its parts. Each view's entities are also a group, `VIEW-id`. A later request, by the AI or typed, reads them to add to a view in its own coordinates, or to balloon another part.

**Typed commands never move what is placed.**
- `VIEWFRAME` adds a view to an existing sheet in the space left, or refuses, saying how much room it needs.
- Moving a view is the drafter's `MOVE` of its group. The review reports a projected view moved out of line.
- Re-composing a sheet means asking for it again. The declaration is regenerated, and the views' contents are drawn again from the request. A view's hand edits are not carried.

### 7. The review checks the structure

The sheet review (#349) gains structural checks:
- geometry outside its view's frame;
- views overlapping;
- a projected view out of line with its parent;
- a section without its marker;
- a balloon without its part;
- a datum used in one view of several that show its axis.

### 8. The boundary, and what validity means

- **The composer reasons in no 3D.** ADR-0040 §2 allows a bounded generator to use implicit 3D geometry to derive its views, and that stands: `PARTVIEWS` may draw a composed sheet's views. What is ruled out is a persistent general-purpose 3D model. The composer itself places and ties 2D views. It does not compute one view from another, because nothing in a composed sheet describes the object in 3D, only the views drawn of it.
- **Structurally valid is not correct.** The composer and the review check that the sheet holds together:
  - the views are placed and aligned;
  - every balloon names a part, every section has its marker;
  - datums agree where used.

  They do not check that the assembly works, that a section shows what a real cut would, or that a bill of materials is complete for what it describes. A sheet can be structurally valid and mechanically wrong. That remains the drafter's judgement, and the AI's replies say so where the request leaves geometry unspecified.

### 9. L5 scores four things apart

The corpus level for composed sheets (#358) judges each case on four scores, and a case passes only when all four do:
1. **Structure.** The views asked for exist, with their kinds and projection; sections are marked; every balloon's part is in the table.
2. **Geometric agreement.** Each datum the case names is at its value in every view that shows its axis, to the drawing's tolerance.
3. **Dimension accuracy.** Each size the request gives is dimensioned, and reads its value.
4. **Print-scale accuracy.** Measured on the sheet, a real length L in a view at ratio r is L·r paper mm. The border is the paper's size × N.

A beautiful sheet that fails any of them fails.

## Alternatives considered

- **More review, no structure.** Let the model keep doing layout, and catch more of it. Rejected as the main path: each finding costs a revision call, and alignment is not something a review makes a model get right.
- **A constraint solver between views.** Relate features across views by constraints and solve them, as ADR-0038 does within a drawing. Rejected: datums, names for numbers, give the agreement that matters without a solver, and keep the scope a sheet composer.
- **Placement at compile time from declared sizes.** The first version of this ADR. Rejected after review: declared extents leave out annotations, so views placed by them collide.
- **Views as viewports on a layout** (ADR-0029, ADR-0030). The professional way to print several scales on one sheet, and it needs no DIMLFAC. Not now: the AI's proposals, the corpus's judge, and the review all work in model space, and a layout puts the sheet where the user at the model tab does not look. Noted for later; the scale contract (§4) maps onto viewports directly, k being a viewport's scale.
- **A generator per kind of sheet.** Rejected: there is no end of kinds.

## Consequences

- **Intent version 15:** `sheet`, `view`, `datum`, `parts`, `balloon`, `dim_chain`, and `notes`; `in` on drawing operations; coordinates that may name a datum; `layer` gains `color` and `linetype`.
- **The engine composes.** `cad-commands` gains the two-pass placement (the measuring run, then the drawing run) as one transaction. The placement function is pure and has its property tests.
- **New commands,** typed as well as compiled to: `SHEET`, `VIEWFRAME`, `SECTIONMARK`, `BALLOON`, `BOMTABLE`, `NOTES`. Each is registered, tested headless, and on a menu.
- **Dimension styles** gain a linear factor, with its DXF group (144).
- **The corpus** gains L5, scored as §9 says, in English and Portuguese, the spillway and the armour among its cases.

## Revisit when

- A composed sheet needs several scales printed exactly: map it onto a layout's viewports (§4 carries over).
- Datums prove too coarse: a feature needs a relation, not a number. Weigh a solver between views then, with evidence.
- Requests ask for views computed from a solid: ADR-0040 §2 applies.
