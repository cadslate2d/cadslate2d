# M18 exit review — Classic Menu Bar

**Date:** 2026-10-18  
**Scope:** the menu bar of DESIGN §5, after AutoCAD 2007/2008's classic workspace, asked for by the user after M17 ([roadmap](../roadmap.md), epic #244, [ADR-0034](../adr/0034-menu-macros.md))  
**Outcome:** Every item is implemented and tested.
- The menu bar has AutoCAD 2007/2008's ten classic menus, in its order. Over a hundred items each run a command as typed, some pausing for the user's answer as AutoCAD's macros do.
- It answers to the keyboard as AutoCAD's does.
- File › New makes an empty drawing.
- An automated scenario drafts a plate through the menus alone and saves it.

The milestone is closed.

## The scenario

`web/e2e/m18-scenario.spec.ts`, in Chromium and Firefox. Every command is started from a menu, by the pointer or by Alt and the letters; only the answers are typed.

1. **File › New**, then **Format › Layer** makes `PLATE` current.
2. **Draw › Rectangle** for the outline and **Draw › Circle › Center, Radius** for a hole, by the pointer. Alt+D, C, 2 (**Draw › Circle › 2 Points**) for the second hole, by the keyboard.
3. Alt+N, L (**Dimension › Linear**) across the bottom.
4. **Draw › Line** for a bracket, its corner rounded by **Modify › Fillet**.
5. **Tools › Inquiry › Distance** across the plate: 116.6190. Then **View › Zoom › Extents**.
6. **File › Save As**. The file holds:
   - the outline, two holes, the dimension, and the fillet's arc;
   - all of it on `PLATE`.

   No line of the log says a command is unknown or an answer refused.

## Done when (epic #244)

| Criterion | Met by | Evidence |
| --- | --- | --- |
| File › New makes an empty drawing, asking first when there are unsaved changes | #245 | `a_new_drawing_is_empty_untitled_and_keeps_the_unit`; the `CadApp` test of NEW; `new-drawing.spec.ts` |
| The row of the visual identity: menus at the left, New, Open, Save, and Export at the right; the panel switches in View; Settings in Tools › Options | #246 | `menubar.spec.ts`; the baseline `menubar-view-zoom.png`; the specs that used the header switches now use the menus |
| Every item a macro over the command line, pausing where AutoCAD's do | #246 | ADR-0034; `macro.test.ts`; the `CadApp` macro tests on the real engine (a pause resumed, a pause dropped, a selection pause with and without a selection) |
| AutoCAD 2007/2008's menus, each item a registered command, greyed when it cannot apply | #247 | `menus.test.ts`: the ten menus in order; every macro up to its first pause runs from a fresh drawing without a refusal; one mnemonic per entry, none twice; Undo, Redo, Paste, and the viewport items greyed; `menus.spec.ts` |
| Alt opens the menus, as in AutoCAD | #248 | `menu-nav.test.ts`, the key-routing tests, `menu-keyboard.spec.ts` |
| A scenario drafts through the menus alone | #249 | `m18-scenario.spec.ts` |

## Decisions

- **A menu item is a macro over the command line** (ADR-0034). The core cannot tell it from typing.
- **A pause waits for the command's next question.** The rest of the macro is typed once the command asks something else. A selection pause waits while objects are asked for, and is skipped when they are already chosen. A paused macro is dropped when its command ends or another starts. This covers Arc › Start, Center, End, Circle › Center, Diameter, and Draw Order without a macro language in the core.
- **Left out, not greyed, when the program lacks it; greyed when it cannot apply now.** A dead item would promise something that is not there. Undo, Redo, Paste, and the viewport items grey out as AutoCAD's do. Paste reads a new flag in the UI state, whether the clipboard holds anything.
- **The header's switches moved into the menus.** The toolbar and panels are checked items of View, and Settings is Tools › Options. The header keeps the brand, as in the visual identity.
- **Only keys the page may have are shown.** The browser keeps Ctrl+N, Ctrl+1, and Ctrl+Shift+C, so File › New, Properties, and Copy with Base Point show no key. Ctrl+P (plot), Ctrl+Shift+S (save as), and Ctrl+A (everything, at the idle prompt) are new, and work in both browsers.
- **The bar never takes the browser's focus.** While its menus have the keyboard they take keys ahead of the command line. Afterwards typing reaches the line again, and the menus' Esc never cancels a command.
- **A new drawing keeps the unit** of the one it replaces. Nothing is left to recover, so the kept drawing goes too.

## What AutoCAD 2007/2008's menus have that ours do not

Left out because the program does not have them, menu by menu:

- **File:** Close, Partial Load, Import, Attach, eTransmit, Publish to Web, Page Setup Manager, Plotter Manager, Plot Style Manager, Plot Preview, Publish, Audit, Recover, Drawing Properties, the recent files, and Exit. A drawing is one per tab, and the browser keeps the files. The plot is a PDF with no plotters or style tables (ADR-0027).
- **Edit:** Paste as Block, Paste as Hyperlink, Paste to Original Coordinates, Paste Special, OLE Links, and Links. The clipboard is the session's, with no OLE.
- **View:**
  - Redraw, Regen, and Regen All: the scene never needs regenerating (ADR-0003).
  - Zoom Realtime, Dynamic, Object, In, and Out, and Pan Realtime: the wheel and the middle button do these.
  - Aerial View, Clean Screen, and tiled and polygonal viewports.
  - The 3D views, cameras, visual styles, render, and motion paths: no 3D (DESIGN §4).
  - The UCS icon (one coordinate system, §4.1).
  - Customizing the toolbars.
- **Insert:** DWG, DWF, DGN, and image references (no xrefs or underlays), Field, Layout from Template, the Layout Wizard, 3D Studio, ACIS, Windows Metafile, OLE Object, and Hyperlink.
- **Format:** Layer Previous, Layer Walk, Change to Current Layer, Copy Objects to New Layer, Layer Match, and freezing in one viewport; Scale List, Table Style, Plot Style, Multiline Style, Thickness, and Drawing Limits.
- **Tools:**
  - Workspaces, Palettes, Spelling, Update Fields, the Block Editor, and in-place editing;
  - Data Extraction, Load Application, scripts, macros, and AutoLISP;
  - the UCS, Geographic Location, CAD Standards, Wizards, Tablet, and Customize;
  - from Inquiry: Radius, Angle, Region/Mass Properties, Time, Status, and Set Variable.
- **Draw:**
  - Multiline, 3D Polyline, Helix, Table, Region, Gradient, surfaces, and modeling;
  - Circle Tan, Tan, Radius and Tan, Tan, Tan;
  - Block › Base;
  - Point › Single Point: `POINT` here repeats until Enter, so it is Multiple Point;
  - the Arc variants `ARC` does not offer (Start, End, …; Center, Start, …; Continue). A macro can only answer what the command asks.
- **Dimension:** Arc Length, Jogged, Dimension Space, Dimension Break, Tolerance, Center Mark, Inspection, Jogged Linear, Oblique, Align Text, Override, Update, and Reassociate.
- **Modify:** Change to ByLayer, the External Reference, Image, and Multiline objects, the Block Attribute Manager, Text scale and justify, Clip, Annotative Object Scale, the 3D and solid operations, and Change Space.
- **Window:** the whole menu. A drawing is one per tab.
- **Help:** Help, New Features Workshop, Additional Resources, and Send Feedback. About is there.

## What testing found

- **A click elsewhere did not close a menu.** The canvas cancels `pointerdown`, so no `mousedown` follows. The bar listens for `pointerdown`.
- **`D` was both Delete and Dogleg.** That was in M17's `MLEADERSTYLE`. The menus' own mnemonics are now checked by a test: one per entry, none twice.
- **The new row takes 32 px from the canvas.** Two tests depended on the canvas's size:
  - The layout baseline was re-recorded.
  - `far-from-origin.spec.ts` drew 600 near lines and called the far plot "in the second chunk". That chunk held 89 of the near lines too, the case ADR-0028 leaves to chunking by locality. It passed by luck at 72 px per unit and failed at 66.7. It now draws 511, as ADR-0028 and the Rust test do.
- **The M13 scenario still flaked now and then.** The cause was the test's own last click, about 2 px from a wall's end, with the first move 407 ms later. The host's 400 ms rest timer from that click fired once OTRACK was on and acquired the end, and the test's deliberate rest then let it go. The program behaves as designed. The test now rests away from the walls first (`aa179da`). Earlier guesses are corrected in `3e533ce`.
- **Ctrl+P plots interactively.** `PLOT` asks for the sheet first, so the key starts it rather than downloading a file.

## Measurements

- The suite: 1,751 Rust tests, 172 web unit tests, and 520 end-to-end runs across Chromium and Firefox.
- The menus' contents test runs each of over a hundred macros on a fresh engine in under a second.
- The machine was heavily loaded by other work during part of the milestone (a load average near 30). Two full e2e runs then missed time limits that pass alone: the M14 scenario's 30 s, and the M7 scenario's 2,000 ms open budget, at 2,034 ms. `5d4c978` was committed after the M14 scenario passed on its own. The later commits' full runs passed whole.

## What follows

Nothing is planned. **M15 (#231)** still holds what needs a person: real GPUs and the text atlas, WebKit, the upstream crash, and the check in AutoCAD and a desktop PDF viewer.

Classic-interface work this milestone did not take up, should drafters ask:
- right-click shortcut menus;
- the grip menu;
- the `ARC` variants that would let Draw › Arc have AutoCAD's full cascade.

The long-term ideas stay on the M15 epic.
