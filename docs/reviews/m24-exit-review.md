# M24 exit review — General-Purpose Drawings

**Date:** 2026-10-27  
**Scope:** the drawings beyond plans and parts that the maintainer asked for on 2026-10-26: façades, cross sections, and isometrics for architecture and mechanical engineering. They are to be drawn by command and through the AI, within DESIGN §4's 2D ([roadmap](../roadmap.md), epic #330, [ADR-0040](../adr/0040-general-purpose-drawings.md)).  
**Outcome:** Every item is implemented and tested, by command and by request, in English and Portuguese. Three things need a person:
- ADR-0040 is *proposed*; the maintainer accepts it here or asks for changes;
- the elevations and sections are of one storey, as the plans are;
- the corpus's recordings are of one run per instruction change.

What M24 delivered, each kind a generator from a description, and nothing 3D kept:
- **Isometric drafting** (#336):
  - `SNAP` Style Isometric: the lattice along 30°, 150°, and 90°, and the grid drawn on those axes;
  - ORTHO along the current plane;
  - `ISOPLANE`, F5, and Ctrl+E, with the plane shown in the status bar;
  - `ELLIPSE` Isocircle;
  - text on a plane by an obliqued style.
- **Elevations** (#337): `ELEVATION` draws a drawn plan's façade from its brief's new `building` heights and roof. The roof is flat behind a parapet, gable, or hip, at a pitch, with eaves.
- **Building sections** (#338): `BSECTION` cuts across a drawn plan:
  - walls cut and hatched, broken at doors and windows;
  - slabs, and the roof's slopes;
  - openings beyond;
  - the stair in section;
  - the cut marked on the plan with its letter.
- **Mechanical views** (#339): `PARTVIEWS` draws a prism's front, top, and right views, or a turned part's front and right views, in first- or third-angle projection:
  - hidden edges dashed and centre lines chained;
  - optionally a section, hatched, with its cutting plane lettered;
  - the part given as JSON, or as a drawn front view and a depth.
- **Title blocks** (#340): `TITLEBLOCK` draws a sheet's border and title block, A4 to A0, in the model at a scale or on a layout in paper millimetres. It draws the ISO projection symbol too.
- **Staircases** (#331, before the milestone was filed): `STAIR` by Blondel's rule, in plan and section, and in a plan's stair room.
- **Through the AI:** intent versions 10 to 13 add `stair`, the isometric operations, `elevation`, `building_section`, `part_views`, `title_block`, and a brief's stair room, storey height, and building. The corpus gains a level, **L4**.

## The scenario

`web/e2e/m24-scenario.spec.ts` runs in Chromium and Firefox. Its requests are answered by the tests' stand-in service from the L4 corpus's recordings of the real model:

1. **A house by command** in millimetres: `FLOORPLAN`, its brief as JSON.
2. **Its front façade by a request**, the plan selected and sent with the user's consent: "Draw the front elevation of this plan with a gable roof at 30 degrees, …". It is accepted. **A section by command**, from the front to the back, hatched.
3. **An isometric top face by a request**, and a hole on it with `ELLIPSE` Isocircle on the isometric snap.
4. **A new drawing:** a block's views by a request, and a title block round them by `TITLEBLOCK`. Then saved and plotted.

## Done when (epic #330)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| An ADR: what is derived from what, and what the IR expresses | #335 | [ADR-0040](../adr/0040-general-purpose-drawings.md): each kind a generator; the line against a 3D model and what would cross it; the IR's operations; L4 |
| Façades and cross sections of a building | #337, #338 | `cad_layout::elevation` and `cad_layout::section`, with unit tests: openings at their heights, every wall cut at its thickness, a pitched roof's height everywhere. `tests/elevation.rs`, `tests/bsection.rs`; `web/e2e/floorplan.spec.ts` |
| Isometric drafting | #336 | `cad_core::iso`: lattice against brute force, isocircles against the circles they draw. `tests/isometric.rs`; `web/e2e/isometric.spec.ts` |
| Mechanical drawings: orthographic views, sections, centre lines, title block | #339, #340 | `cad_geometry::views`: views agreeing, hidden edges exactly where covered, the hatch only in cut material. `titleblock.rs`'s margins and symbol. `tests/partviews.rs`, `tests/titleblock.rs`; `web/e2e/partviews.spec.ts` |
| The intent IR and the corpus grown level by level, each kind in English and Portuguese | all | Intent version 13 (`test-data/intent/schema-v13.json`). `test-data/ai/l4`: 18 requests, recorded at 100% in both languages; L1–L3 recorded again at 100% |
| A scenario and a review | #341 | `web/e2e/m24-scenario.spec.ts`; this review |

## What it does not do, and why

- **One storey.** The plans have one floor, so the elevations and sections show one storey under its roof. A stair is drawn to the storey above, but that storey is not drawn. Several floors would be a brief per floor: a change to the layout generator, not to the views.
- **A section shows what it cuts and the far walls' openings.** Interior walls beyond are not drawn. A stair appears in section when the cut runs up its first flight; a U stair's return flight beyond is not drawn.
- **The parts are prisms of straight edges with holes right through along an axis, and turned parts of plain steps with a bore.** Arcs in a profile, fillets between faces, blind or angled holes, grooves, and threads are left out. Each would need its own closed form, and ADR-0040 §2 says when that becomes a solid modeller and needs a new ADR.
- **Isometric dimensioning is not done.** Isometric text is a style obliqued 30° and turned 30°, set up by hand.
- **Plans are deterministic on one platform only** (ADR-0039). The WASM app and the native tests can lay out the same brief differently. That is why each test reads positions from the plan it drew, never from another platform's.

## Measurements

**The AI corpus** at intent version 13 (`test-data/ai/README.md`):

| Level | en | pt |
| --- | --- | --- |
| L1 | 37/37 | 37/37 |
| L2 | 29/29 | 29/29 |
| L3 | 26/26 | 26/26 |
| L4 | 9/9 | 9/9 |

Each re-record found a few answers that slipped and were right when asked again. They are listed in the corpus README. After the instructions began quoting Portuguese drafting terms, three English requests were answered in Portuguese. One line put that right: the next full run was 100% everywhere.

**The layout search** with stair rooms: two briefs were added (a stair hall, and a stair room too small). The reliability study then had 3,371 of 3,400 runs as expected; every miss is the hard brief's, as before.

## The whole suite

`make e2e`: 606 flows pass in Chromium and Firefox. The sweep found one failure older than M24. Since the tab took the logo as its icon (6f95232), Firefox fetches `apple-touch-icon.png`, and the M0 user story allowed only scripts, styles, WASM, and pages as same-origin static requests. Icons are static files too, and the story now allows them.

## Registry sweep

New commands, each registered, in the README's table, on a menu, and reachable by intent:

| Command | Menu | Intent operation |
| --- | --- | --- |
| `STAIR` | Draw › Building › Stair... | `stair` |
| `ISOPLANE` | Tools › Drafting Settings › Isometric Plane (F5) | — (the isometric operations take axis coordinates) |
| `ELEVATION` | Draw › Building › Elevation... | `elevation` |
| `BSECTION` | Draw › Building › Section... | `building_section` |
| `PARTVIEWS` | Draw › Orthographic Views... | `part_views` |
| `TITLEBLOCK` | Insert › Title Block... | `title_block` |

`SNAP` gained Style and `ELLIPSE` gained Isocircle, the latter greyed on the menu while the snap is standard.

## For the maintainer

- Accept ADR-0040, or say what to change.
- Try a façade and a section of a plan of your own, and a part of your own: Draw › Building, and Draw › Orthographic Views.
