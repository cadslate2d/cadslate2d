# ADR-0020: Dimensions — definition points and a style, laid out by pure functions whenever asked

**Status:** Accepted (2026-10-06)  
**Date:** 2026-10-06  
**Kind:** Bootstrap (settles the model DESIGN §34 and §19.6 constrain before the first dimension command; no LOCKED item changes)  
**Design refs:** §11, §19.2, §19.6, §34; ADR-0013, ADR-0016, ADR-0017

## Context

A dimension is drawn as extension lines, a dimension line or arc, two arrowheads, and a text saying what it measures — a dozen segments and a few hundred strokes — but what it *is* is a few points, a style, and perhaps words of the drafter's. §34 orders the work: text metrics (#161), then styles (#162), then the layout engine, then rendering. §19.6 says what a style change must be: one semantic change, however many dimensions it draws. Four questions have to be answered before the first command:

1. What does a dimension store, and what does it compute?
2. Where does the geometry live, and when is it computed?
3. What are a dimension's grips, and what does each hold fixed?
4. How does a style change reach every dimension drawn by the style, within the one-change rule?

## Decision

### 1. A dimension is what it measures, where it is placed, a style, and perhaps its own words

`DimensionEntity` holds a `Measure` — two points and a direction for a linear dimension (0 horizontal, π/2 vertical, any angle rotated), two points for an aligned one, a vertex and two points for an angle, a centre and radius for a radius or diameter — a *placement point*, a `DimStyleId`, and an optional text override. The placement point is the one thing beside the measure that the drafter chooses: the point the dimension line passes through, the point on the dimension arc, or where the text of a radius sits, which also sets the leader's angle. Nothing else is stored: not the extension lines, not the arrowheads, not the text's position or its words.

A dimension that cannot be laid out — coincident points, a point of an angle on its vertex, a radius that is not positive, a coordinate that is not a number — is refused by the transaction (`TransactionError::BadDimension`), as is one whose style the table lacks.

### 2. The layout is a pure function of the entity and the style, computed when asked

`DimensionEntity::layout(&DimStyle) -> DimensionLayout` gives the lines, the arc, the arrowheads, the text (a `TextEntity`, centred, reading the right way up), and the measured value. It is called whenever anyone needs the geometry — bounds for the spatial index, the distance for picking, what crosses a window, what the renderer draws, what EXPLODE makes — and never cached. The rules it follows, for every kind:

- Extension lines start the style's offset away from the measured points and run its overshoot past the dimension line, perpendicular to it (radial, for an angle); a point on the line, or beyond the arc, needs none.
- Arrowheads sit with their tips on the dimension line at its ends, pointing outwards, a closed triangle the style's arrow size long and a sixth as wide as long; when the line is too short for two of them they go outside, pointing in, and the line is carried on two arrow lengths past each end.
- The text is the value shown to the style's decimals with its suffix — degrees with `°`, `R` before a radius, `Ø` before a diameter — or the drafter's words; it is placed above the middle of the line, its box clear of the line by the style's gap and never by less than half an arrowhead's width; when it does not fit between the arrowheads it goes past the second end, with the line carried on beneath it. A radius or diameter text is placed along the leader where the drafter put it, clamped clear of the arrowheads, or outside the circle at the end of a leader when it does not fit inside.
- The text reads at the dimension line's angle brought into (−π/2, π/2]: never upside down, whatever the direction or a mirror does.

The geometry that knows nothing of styles — arrowheads, the foot of a perpendicular, the readable angle — is in `cad-geometry`; the layout, which needs the style and the font's metrics (`cad-text`, #161), is in `cad-document`.

### 3. Grips are the points: what is measured, then the placement

A dimension's grips are its measured points (`GripKind::Definition`) followed by its placement point (`GripKind::Placement`) — two and one for a linear or aligned dimension, three and one for an angle, the centre and the placement for a radius or diameter. Dragging a measured point measures anew: the dimension's value changes, the style and the words do not. Dragging the placement moves the dimension line, arc, or text; the value does not change. A radius grips its centre and placement but not its radius: that is re-dimensioned, not dragged. STRETCH follows the same points, one by one. A grip that would make a dimension that cannot be laid out is refused, as a line dragged to no length is (ADR-0010).

### 4. A style change is one row; the index entries follow in `apply`

`Entity::bounds` and its siblings take `Tables` — the block table and the dimension style table, `Drawing::tables()` — in place of the block table ADR-0017 threaded through them, so that a reference and a dimension are resolved the same way: through the table, with nothing derived stored in the entity. Replacing a dimension style is `TableChange::Replace` on the one row. `Drawing::apply`, having applied it, finds every dimension drawn by the style, updates its index entry from the layout as it now is, and reports it as replaced; the shown revision changes, so the scene is rebuilt — exactly as a block redefinition is handled. A style with ten thousand dimensions is one history entry, ten thousand index updates, and one scene rebuild. A style a dimension is drawn by cannot be removed (`DimStyleInUse`).

### 5. A dimension is an annotation, not geometry of the drawing

Like a text (ADR-0016), a dimension has no primitives: nothing snaps to, intersects, trims against, or extends to its lines, and the curve commands refuse it by name. It is picked by its lines, arrowheads, and text box. It offers to snap: the points it measures (as endpoints), its vertex or centre (as a centre), and its text's insertion point. It is drawn continuous whatever its linetype says, as a text is. EXPLODE turns it into what its style laid out — lines, closed polylines for the arrowheads, an arc, a text — with the dimension's properties; the parts are then ordinary geometry.

## Evidence

The layout holds under property tests over every kind, random points to ±1000, and random styles (`crates/cad-document/src/dimension.rs`):

- `the_value_is_what_the_points_measure_whatever_the_style`: the value is the projected distance, the distance, the angle between the rays (or its reflex), the radius, or twice it, for any style; the text contains the value as the style formats it.
- `extension_lines_stand_square_to_the_dimension_line_and_arrowheads_lie_on_it`: every extension line is perpendicular to the dimension line (radial, for an angle); every arrowhead's tip lies on the line, arc, or circle, and is the style's size long.
- `the_text_box_overlaps_no_arrowhead_and_the_bounds_hold_everything`: the text's box shares no area with any arrowhead; the bounds hold every line end, arc point, arrowhead corner, and text corner; the text reads the right way up.
- `moving_a_dimension_moves_its_layout`: the layout of a moved dimension is the moved layout, value unchanged.

Unit tests fix the Standard layout of a horizontal dimension, the vertical and aligned readings of the same points, the short line (arrowheads outside, then the text), the angle on either side of its vertex with and without extension lines, the radius and diameter inside and outside the circle, the refusals, the grips, the transforms, and EXPLODE. Through the session (`crates/cad-commands/src/tests/dimension.rs`): picking by line, text, and extension line and not by the space between; the grips' kinds; the Properties panel's rows; a style change redrawing two dimensions in one step, undone in one, with picking following the new layout; a used style refused for deletion; MOVE, OFFSET's refusal, EXPLODE and its undo. In the scene (`crates/cad-render/tests/render_boundary.rs`): a dimension's segments and arc; the revision unchanged by the pointer and changed by a style change. Snapping (`crates/cad-commands/src/snap.rs`): points, centre, and text offered, nothing on the lines.

## Alternatives considered

- **Storing the layout in the entity.** Fast to read and wrong after a style change unless every dimension is rewritten — the one-change-per-dimension §19.6 forbids. Rejected; the layout is computed, and the one derived store that must be current, the spatial index, is updated in `apply`, as for blocks.
- **A separate text entity for the dimension's words.** Two entities for one dimension, kept in step by every command. Rejected; the text is part of the layout, and the override is a field.
- **Keeping `&BlockTable` and reading the style some other way.** A second parameter everywhere, or the style looked up through a global. Rejected; one `Tables` context names everything geometry is resolved against, and the compiler found every site.
- **Associativity**: a dimension that follows the object it measured. Its points are its own (#163 scope); associativity needs the object's identity in the dimension and a reconciliation pass, which nothing else in the model has yet.
- **Filled arrowheads.** The renderer draws lines (ADR-0003); a filled triangle is the hatch question (#167). The arrowhead is a closed outline until then.

## Consequences

- `Entity::Dimension` joins the enum; `Tables` replaces `&BlockTable` in `Entity::bounds`, `distance_to`, `primitives`, `intersections`, `is_inside`, `crosses`, `exploded`, `PackedFrame::push_entity`, and `entity_snap_points`; `Tables::none()` stands where `empty_blocks()` did.
- `TransactionError::BadDimension`; `DimStyleInUse` now also means a dimension is drawn by the style.
- The stroke font gains `°` and `Ø`, built from its own `0` and `O`, so a dimension's text is drawn and not boxed.
- DXF writes a `DIMENSION` with its kind, points, style, text, and value; the anonymous block that readers which do not lay dimensions out expect, and reading `DIMENSION`, are #166.
- The commands (#164, #165) are state machines over this entity: a preview is `layout` of the dimension the click would make.

## Revisit when

- Associative dimensions are wanted: the measure then names entities, and a reconciliation pass after every transaction re-measures.
- A drawing with tens of thousands of dimensions in one style makes the per-change re-layout the slow step; a reverse index from style to dimensions would make it proportional to the dimensions touched.
- Ordinate, arc-length, or leader dimensions, tolerances, or alternate units are wanted: new `Measure` variants and layout rules, within this model.
