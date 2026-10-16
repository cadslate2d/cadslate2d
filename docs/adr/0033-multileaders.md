# ADR-0033: A multileader stores its leaders, landing, and content; its style lays it out

**Status:** Accepted (2026-10-16)  
**Date:** 2026-10-16  
**Kind:** Design (#240)  
**Design refs:** §34; ADR-0014, ADR-0017, ADR-0020, ADR-0021

## Context

`LEADER` (#205) is one line to a note, laid out by a dimension style. Production drawings annotate with multileaders instead (DXF `MULTILEADER`). A multileader has:
- several leader lines to one note;
- a horizontal *dogleg* at the landing, so the note reads square to it;
- a note that is text or a block — a balloon, a tag;
- a style table of its own (`MLEADERSTYLE`).

ADR-0020 settled how annotation is kept: what defines it is stored, and what is drawn is laid out from a style row whenever it is needed. So a style change is one row changing, and nothing derived goes stale. The multileader should follow that rule rather than store its arrowheads and text placement.

## Decision

1. **What is stored.** `MLeaderEntity` holds:
   - its leaders: each a list of points from the arrowhead's tip to the last bend before the landing, the line then running on to the landing;
   - the landing, the one point every leader meets;
   - its content: text, a block reference by its block's ID, or nothing;
   - its style, by the ID of a row of the multileader style table.

   There is one landing. AutoCAD's multileader may have two leader roots, one on each side of its note. A file's second root is folded into the first: its leaders come to the same landing.
2. **What the style lays out.** A row of the `MLEADERSTYLE` table holds the arrowhead size, the landing gap, the dogleg length (none when zero), the text height, and a default block and its scale. `MLeaderEntity::layout(style, blocks)` draws:
   - each leader with its arrowhead at its tip, pointing back along its first segment;
   - the dogleg from the landing, horizontally toward the side away from the leaders;
   - the note past the dogleg's end by the gap: text with its middle on the dogleg's line, reading away from it, or a block whose nearer edge touches the dogleg's end.

   Nothing of the layout is stored: `Entity::bounds`, picking, snapping, and drawing lay it out from `Tables`, which gains the multileader style table, as dimensions do (ADR-0020). A style row replaced re-indexes the multileaders drawn by it in the same `apply`, as a dimension style's does.
3. **One walk.** `walk_entity` draws the layout's lines, the arrowheads' outlines, and the text's strokes; a block's content is walked through the block table, as a reference is (ADR-0017). Screen and page see the same thing; no new instance kind or style rule is needed (ADR-0014, ADR-0021).
4. **Editing.**
   - The grips are each leader's points and the landing.
   - A transform moves every point; the dogleg follows from where the leaders are.
   - `MLEADEREDIT` adds and removes leaders. A multileader keeps at least one.
5. **The file.** DXF `MULTILEADER` carries the same: the context data's root holds the last leader point (our landing) and the dogleg, its leader lines hold the vertices, and the content is an MTEXT or a block by handle. The style is a `MLEADERSTYLE` object. What the drawing does not hold is reported (#242).

## Alternatives considered

- **A `LEADER` with a list of lines.** Smaller, but the dimension style it is drawn by has no dogleg, block content, or default block. A file's `MLEADERSTYLE` rows would have nowhere to go, so a round trip would lose them.
- **Storing the laid-out geometry, as DXF's context data does.** Every style change would have to rewrite every multileader drawn by it, the drift ADR-0020 exists to prevent.
- **Two roots.** It is faithful to AutoCAD, but no command here makes a second root. It would double every layout rule for a case files rarely have, so a second root is folded and reported.

## Consequences

- Multileaders are annotation: they are not cut by `TRIM`, offset, or joined, and their own lines are not snapped to, as with dimensions (ADR-0020).
- A multileader with a block draws the block's current definition, so redefining the block redraws every balloon (ADR-0017).
- The style table is one more table with a permanent `Standard` row, and the history property test covers it.

## Revisit when

- Drafters need a note between two roots, with leaders arriving from both sides.
- Spline leaders, which DXF allows, are wanted rather than read straight.
