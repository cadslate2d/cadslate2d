# Visual baselines

Small clipped screenshots that end-to-end tests compare against (`toHaveScreenshot`). They catch a marker or grip changing shape, colour, or size by accident.

| Files | Recorded by | Shows |
| --- | --- | --- |
| `grip-idle.png`, `grip-hot.png` | `web/e2e/grips.spec.ts` | A line's endpoint grip, unhovered and under the cursor |
| `snap-marker-<mode>.png` | `web/e2e/snap.spec.ts` | The object-snap marker of each mode, 28×28 CSS pixels around the snap point, with the crosshair beside it |
| `linetype-<name>.png` | `web/e2e/linetype.spec.ts` | Each built-in linetype at `LTSCALE` 20 on a line and on a half-circle arc, 180×100 CSS pixels (ADR-0015) |

Rules:

- Baselines are compared in Chromium only, with software WebGL — the configuration CI runs. Other engines antialias differently.
- A comparison tolerates a few dozen differing pixels (driver noise); a different glyph differs by far more.
- Re-record deliberately, then look at the images before committing them:

  ```bash
  E2E_BROWSERS=chromium pnpm --dir web exec playwright test --update-snapshots
  ```
