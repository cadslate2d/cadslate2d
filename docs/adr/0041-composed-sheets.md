# ADR-0041: A sheet is composed by operations that place and tie its parts; the model draws inside them

**Status:** Accepted (2026-10-27, by the maintainer: "approved, proceed"). Asked for on 2026-10-27 ("proceed"), after the Iron Man and spillway sheets.

Revised twice the same day after the maintainer's reviews.
- **The first** accepted the direction and asked for six changes:
  - shared references (§3);
  - the scale contract (§4);
  - placement that knows the annotations (§2);
  - projection declared precisely (§1);
  - references validated and the sheet kept (§6, §7);
  - the boundary stated rightly (§8);
  - and L5 scored four ways (§9).
- **The second** (8.5/10) asked for three more, the first two blocking:
  - signed orientation transforms, and alignment by reference coordinates (§1);
  - measurement of what will be drawn, at its final scale (§2);
  - tables and notes sized in paper (§1).

  It also made four smaller corrections, taken in: datum coverage (§7), section cuts (§6), LTSCALE on a drawing that has other content (§4), and how datums are kept (§3, §6).

  The maintainer approved this revision.

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

### 1. The sheet's structure is declared, in one object frame

**The object frame.** Everything a composed sheet shows is of one object, described in one right-handed frame of real units:
- **width** (W), positive to the right as the front view shows it;
- **depth** (D), positive away from the front, the front face at D = 0;
- **height** (H), positive up.

The origin is the object's reference point: wherever the request puts 0, 0, 0, and by default the lowest, leftmost, frontmost corner.

**`sheet`**: `paper`, `portrait`, `scale` N of 1:N, `projection` (`first` or `third` angle), and the title block's fields. It draws the title block and defines the drawing area inside the border, less the block.

**`view`** has a unique `id` (letters, digits, `-`) and a `title`, and is one of:
- **Principal**: the front, `looking` `from_front`. One per sheet.
- **Projected**: `of` a parent view, `looking` `from_left`, `from_right`, `from_above`, `from_below`, or `from_behind`.
  - The slot follows the sheet's projection. In third angle a view from the left goes left of its parent and a view from above goes above. In first angle each goes on the opposite side.
  - A view from behind goes beyond the side view, on the far side of whichever side view its row has.
  - A projected view takes its parent's scale; another is refused.
- **Section** or **detail**: free views at their own `ratio` (paper : real, such as `2` for 2:1).
  - A section is `looking` as its arrows point (§5).
  - A detail is `looking` as the view it enlarges.
- **Table** or **notes**: free views of a `size` in **paper millimetres**, with no ratio. They hold paper objects, a bill of materials or lines of text, whose column widths and wrapping do not depend on the size of what is drawn. Their extent on the sheet is that size × N model units.

**Each orientation maps the object frame to the view's own coordinates (u, v)**, signed:

| `looking` | u (right on the page) | v (up on the page) |
| --- | --- | --- |
| `from_front` | +W | +H |
| `from_behind` | −W | +H |
| `from_left` | −D | +H |
| `from_right` | +D | +H |
| `from_above` | +W | +D |
| `from_below` | +W | −D |

Every view's (u, v) origin is the image of the object's origin. A feature at width +200 is at u = +200 in the front view and at u = −200 in the rear view. A feature at depth 170 is at u = −170 from the left and at u = +170 from the right.

An operation inside a view gives its coordinates in that view's (u, v). A coordinate given as a datum (§3) is mapped through the table, sign included.

**Alignment is by reference coordinates, not boxes.** Projected neighbours share an axis of the object frame:
- a row (front, side, rear) shares H, so the image of H = 0 is at the same height on the sheet in each;
- a column (top, front, bottom) shares W, so the image of W = 0 is at the same distance across.

Boxes, the measured extents of §2, decide only the spacing along the other axis, and whether the views fit.

Projected views form a tree rooted at the principal view. The following are refused, each saying which:
- a cycle;
- an unknown parent;
- two views in one slot;
- a projected view at a scale other than its parent's;
- a second principal view;
- a table or notes view given a ratio.

**Position is not geometry.** A view placed from the left of the front is positioned and oriented as that view is in the projection. The program does not draw what is seen from the left: the model does, inside it. §3 makes the views agree where it matters.

### 2. What will be drawn is measured, then placed, and nothing is drawn twice

Declared extents are not rendered extents: dimensions, balloons, labels, and section letters reach past the geometry. And a detail view's size depends on its scale. So the engine places views from what they will actually draw, at its final size, in four steps.

1. **Resolve.** Each view's operations are made final, at the view's own origin, before anything runs:
   - datums are replaced by their signed values (§1, §3);
   - geometry is scaled by the view's factor k (§4);
   - text heights, dimension styles (overall scale N, linear factor 1/k), and hatch scales are set as §4 says.

   What is left is a list of ordinary operations whose only difference from the final drawing is where the view's origin is.
2. **Run once, and measure.** The resolved operations run once, through their commands as typed, on a scratch copy of the drawing's tables at the view's origin. The view's title is added. The entities they made are measured: `Entity::bounds` with the tables, so dimension text and leaders count.
3. **Place.** A pure function of the sheet, the projection tree, the reference alignment (§1), and the measured boxes gives each view its origin on the sheet.
   - Projected views go in their slots with a gap of 10 paper mm between boxes.
   - Free views are packed into the space left, largest first, in rows.
   - Nothing overlaps. A view that cannot be placed refuses the sheet, saying which view and how much room it lacks.
4. **Bring in.** The entities made in step 2 are brought into the drawing as the clipboard brings objects in (`TransactionBuilder::bring_in`, rows matched by name). Each view's entities are moved by the translation to its placed origin, a pure translation, so what was measured is exactly what is drawn. This is one transaction, one undo.

The placement function has property tests:
- projected neighbours share their reference coordinate exactly;
- no two placed boxes overlap;
- every box is inside the drawing area.

### 3. Shared references: datums

A **`datum`** names a value of the object, in real units:
- `name`;
- `axis`: `width`, `depth`, or `height`;
- `value`;
- optionally `views`: the ids of the views the feature it marks is meant to appear in.

For example, the shoulder at height 1450, the reactor's centre at width 0, the chest's front face at depth 0.

Any coordinate of an operation inside a view may be given as `{"datum": "shoulder"}` instead of a number. Which datums a coordinate may name follows the view's orientation (§1):
- u of a front or rear view is width, v is height;
- u of a side view is depth, v is height;
- u of a top or bottom view is width, v is depth.

The datum's value is mapped with the sign the table gives. A datum on an axis the view does not show is refused.

**Kept as metadata, not as constraints.** Datums are resolved into numbers when the document runs. They are kept in the sheet's description (§6), so a later request can draw at the same datums. They are never live: editing a line drawn at a datum moves that line only (ADR-0040 §1).

**What is guaranteed, and what is not.** A feature drawn at a datum in two views is at the same place in both: one number, used twice, with the right sign. A feature drawn with raw numbers in two views is not checked against itself. Agreement across views is guaranteed exactly as far as datums are used. The service's instructions ask the model to declare a datum for every size the request gives and every feature more than one view shows, with `views` naming them.

There is no constraint solver between views, and none is needed: a datum is a name for a number.

### 4. The scale contract

Units:
- **Real units**: the drawing's unit (`mm`). A view's geometry is given in real units, in its own (u, v).
- **Paper millimetres**: what is printed.
- **Model units**: where entities are stored. The sheet is 1:N, so one paper millimetre is N model units: the border is `paper size × N`.

A view at ratio r (paper : real) shows a real length L as L·r paper mm, which is L·r·N model units. So the view's **geometry factor is k = r·N**. A view at the sheet's own scale has r = 1/N, so k = 1, and its geometry is stored true size, as #348 asked.

Transformation order, for each point of a view's geometry:
1. resolve datums, signed;
2. scale by k about the view's origin;
3. translate to the view's placed origin, after measurement (§2).

Nothing is rotated: orientation is the (u, v) mapping of §1, applied by the model and the datums, not a transform of drawn geometry.

What is sized by paper, not by k, and set when the operations are resolved, before measuring:
- **Text, dimension text and arrows, balloons, and labels** are given in paper millimetres. They are multiplied by N, so they print the same size in every view, whatever its ratio.
- **Dimensions.** A view's dimensions use a dimension style made for it: overall scale N and **linear factor 1/k** (DIMLFAC, DXF group 144). Their numbers read true size: in a 2:1 detail of a 1:3 sheet, k = 6, and a 10 mm feature drawn 60 units long reads 10.
- **Hatch** spacing is set for paper: a view's hatch scale is multiplied by N, not k.
- **Linetypes.**
  - **On a drawing with nothing else in it**, the composer sets LTSCALE to N, so dashes print alike in every view.
  - **On a drawing that already holds objects with a dashed linetype** under another LTSCALE, the composer leaves LTSCALE alone, and the reply says the sheet's dashes print at the drawing's scale. Changing a global setting under the drafter's own objects would be a silent edit. A linetype scale per object (CELTSCALE) would remove this case; it is not in the program and not in this milestone.

### 5. What a sheet's parts refer to, they refer to by name

- **`section`** is a section view with `cut`: a parent view's `id` and two points in that view's (u, v), and `looking` as its arrows point. The program draws the cutting-plane line, the arrows, and the letters in the parent, and titles the section ("SECTION A-A"). Letters are given in order, skipping I, O, and Q.
- **`parts`** declares the bill of materials:
  - a unique positive `number`;
  - `name`;
  - a positive integer `quantity`;
  - an optional `material`.

  A **`table`** view draws it, columns sized in paper millimetres.
- **`balloon`**: a `part` number, a view, and a point there. It is drawn with a leader and a circle in paper millimetres; balloons are kept clear of each other by the placement of their circles.
- **`dim_chain`**: points along one side of a view, chained, offset from the view's geometry by the program (DIMCONTINUE, as typed).
- **`notes`**: lines of text in a `notes` view, wrapped to its paper width.

### 6. References are validated; the sheet is kept

The following are refused, each saying which:
- a duplicate view id;
- an unknown view, parent, or part;
- a cycle;
- a section whose cut does not cross its parent's geometry (the cutting line may, and usually does, reach past the outline; it must intersect what it cuts);
- a section letter used twice;
- a balloon for a part not declared;
- a part number that is not unique, or not positive;
- a quantity that is not a positive integer;
- a datum unknown, on an axis the view does not show, or naming in `views` a view that does not exist.

A composed sheet is kept as ADR-0040 keeps a generator's description: a group `SHEET-n` whose description holds the declaration as metadata:
- the sheet;
- its views, with their placed origins, orientations, ratios, and geometry factors;
- its datums;
- its parts.

Each view's entities are also a group, `VIEW-id`. A later request, by the AI or typed, reads them to add to a view in its own coordinates, at its datums, or to balloon another part. None of it is live; it is a record of how the sheet was made.

**Typed commands never move what is placed.**
- `VIEWFRAME` adds a view to an existing sheet in the space left, or refuses, saying how much room it needs.
- Moving a view is the drafter's `MOVE` of its group. The review reports a projected view moved out of line.
- Re-composing a sheet means asking for it again. The declaration is regenerated, and the views' contents are drawn again from the request. A view's hand edits are not carried.

### 7. The review checks the structure

The sheet review (#349) gains structural checks:
- geometry outside its view's box;
- views overlapping;
- a projected view out of line with its parent, by reference coordinates;
- a section without its marker;
- a balloon without its part;
- a datum declared with `views` and not used in one of them.

A datum used in one view only, with no `views` declared, is not reported. The feature may genuinely be hidden or absent in the others.

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

## As built

Recorded as each issue lands. Where the code chose among what the decision left open:
- **#351.** The sheet is the title block, `title_block`, already in the IR: it keeps paper, scale, and projection, so there is no separate `sheet` operation.
  - A view's drawing operations are its own `operations`, not an `in` field on every operation. The engine then gets a view's contents as one unit to measure, and sixty operations keep their shape.
  - A run of consecutive views compiles to each view's operations, drawn at its own origin as typed, then one `VIEWFRAME` placing them together. `VIEWFRAME` is the command a drafter types too: one view or several, never moving what is placed.
  - A principal view's block is centred in the drawing area where it fits, so views added later have room on every side.
  - The two passes of §2 are this: run as typed at the view's origin, measure what was made, place it, and move it by a pure translation.
- **#353.** A section is a placement of `VIEWFRAME` (and `placed: "section"` with its `cut`, intent version 17), not a separate SECTIONMARK command. Its marker is drawn in the same transaction as the views: the cutting line on `SECTION-MARKS` in CENTER, an arrow and its letter at each end, a group `SECTION-A`.
  - A parent placed with its section is measured with its marker. A marker on a parent placed earlier is refused if it would run into another view.
  - The cut must cross a line, arc, or circle the parent draws, not its text or dimensions; its ends may lie outside.
- **#354.** One operation, `bill_of_materials`, declares the parts and draws the table (`BOMTABLE`), placed as a free view whose group keeps the parts. That is simpler for the model than a `parts` declaration and a separate `table` view.
  - `balloon` (`BALLOON`) names a part by number and gives its point and the balloon's place in a view's own coordinates. A new answer kind resolves them once the view is placed (intent version 18).
  - A balloon is moved out along its leader until it clears the others.
- **#355.** `dim_chain` is DIMLINEAR then DIMCONTINUE, as typed (intent version 19). Its dimension line is a new answer kind the engine resolves: 10 paper mm beyond everything its view has drawn on that side, so a second chain stands beyond the first.
  - `notes` is the `NOTES` command: MTEXT wrapped to a width in paper mm, placed as a free view.
- **#352.** A free view's `ratio` (intent version 16) scales its geometry by k about its origin before it is measured. Text keeps its height and a hatch its pattern scale. Each of its dimensions takes a style made from its own: `<view>-<style>`, linear factor ÷ k, overall scale N, measured with that style before anything is committed. Its title says its scale, "DETAIL A (2:1)". The dimension style's linear factor is DIMLFAC (144), read and written; an angle is not scaled by it.

## Alternatives considered

- **More review, no structure.** Let the model keep doing layout, and catch more of it. Rejected as the main path: each finding costs a revision call, and alignment is not something a review makes a model get right.
- **A constraint solver between views.** Relate features across views by constraints and solve them, as ADR-0038 does within a drawing. Rejected: datums, names for numbers, give the agreement that matters without a solver, and keep the scope a sheet composer.
- **Placement at compile time from declared sizes.** The first version of this ADR. Rejected after review: declared extents leave out annotations, so views placed by them collide.
- **Measuring, then running the commands again at the placed origin.** The first revision. Rejected after the second review: the two runs could differ, and a detail view measured before scaling is packed too small. One run, measured, and moved, cannot differ from itself.
- **Views as viewports on a layout** (ADR-0029, ADR-0030). The professional way to print several scales on one sheet, and it needs no DIMLFAC. Not now: the AI's proposals, the corpus's judge, and the review all work in model space, and a layout puts the sheet where the user at the model tab does not look. Noted for later; the scale contract (§4) maps onto viewports directly, k being a viewport's scale.
- **A generator per kind of sheet.** Rejected: there is no end of kinds.

## Consequences

- **Intent version 15:** `sheet`, `view`, `datum`, `parts`, `balloon`, `dim_chain`, and `notes`; `in` on drawing operations; coordinates that may name a datum; `layer` gains `color` and `linetype`.
- **The engine composes.** `cad-commands` gains the composition: resolve, run once and measure, place, bring in, as one transaction. The placement function is pure and has its property tests.
- **New commands,** typed as well as compiled to: `SHEET`, `VIEWFRAME`, `SECTIONMARK`, `BALLOON`, `BOMTABLE`, `NOTES`. Each is registered, tested headless, and on a menu.
- **Dimension styles** gain a linear factor, with its DXF group (144).
- **The corpus** gains L5, scored as §9 says, in English and Portuguese, the spillway and the armour among its cases.

## Revisit when

- A composed sheet needs several scales printed exactly: map it onto a layout's viewports (§4 carries over).
- Datums prove too coarse: a feature needs a relation, not a number. Weigh a solver between views then, with evidence.
- Requests ask for views computed from a solid: ADR-0040 §2 applies.
