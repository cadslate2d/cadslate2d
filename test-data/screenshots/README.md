# Visual baselines

Small clipped screenshots that end-to-end tests compare against (`toHaveScreenshot`). They catch a marker or grip changing shape, colour, or size by accident.

| Files | Recorded by | Shows |
| --- | --- | --- |
| `grip-idle.png`, `grip-hot.png` | `web/e2e/grips.spec.ts` | A line's endpoint grip, unhovered and under the cursor |
| `snap-marker-<mode>.png` | `web/e2e/snap.spec.ts` | The object-snap marker of each mode, 28×28 CSS pixels around the snap point, with the crosshair beside it |
| `linetype-<name>.png` | `web/e2e/linetype.spec.ts` | Each built-in linetype at `LTSCALE` 20 on a line and on a half-circle arc, 180×100 CSS pixels (ADR-0015) |
| `text-zoom-1.png`, `text-zoom-in.png`, `text-turned.png` | `web/e2e/text.spec.ts` | A line of text in the stroke font at the default zoom, four times closer, and a second text turned 30° (ADR-0016) |
| `hatch-ansi31.png` | `web/e2e/hatch.spec.ts` | ANSI31 at scale 2 in a 140×80 rectangle with a circular hole, 160×100 CSS pixels (#168) |
| `joins-025mm.png`, `joins-070mm.png`, `joins-211mm.png` | `web/e2e/strokes.spec.ts` | A polyline with a sharp (37°), a shallow (15°), and a reversing join at lineweights 0.25, 0.70, and 2.11 mm with LWT on, 200×120 CSS pixels; the test also holds that no pixel is brighter than the stroke's colour, so no join is drawn twice (#174) |
| `joins-dashed.png` | `web/e2e/strokes.spec.ts` | A Dashed polyline at `LTSCALE` 20 turning three corners, 170×110 CSS pixels: the pattern runs on through each vertex, and a dash that turns a corner is two capsules meeting there |
| `hatch-dots.png` | `web/e2e/strokes.spec.ts` | DOTS at scale 4 in a 140×80 rectangle, 160×100 CSS pixels: each dot a segment of no length, drawn as one round cap |
| `joins-selected.png` | `web/e2e/strokes.spec.ts` | The joins polyline at 0.70 mm selected: the highlight over it and its vertex grips |

Rules:

- Baselines are compared in Chromium only, with software WebGL — the configuration CI runs. Other engines antialias differently.
- A comparison tolerates a few dozen differing pixels (driver noise); a different glyph differs by far more.
- Re-record deliberately, then look at the images before committing them:

  ```bash
  E2E_BROWSERS=chromium pnpm --dir web exec playwright test --update-snapshots
  ```
