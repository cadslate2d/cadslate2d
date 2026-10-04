# Visual baselines

Small clipped screenshots that end-to-end tests compare against (`toHaveScreenshot`). They catch a marker or grip changing shape, colour, or size by accident.

| Files | Recorded by | Shows |
| --- | --- | --- |
| `snap-marker-<mode>.png` | `web/e2e/snap.spec.ts` | The object-snap marker of each mode, 28×28 CSS pixels around the snap point, with the crosshair beside it |

Rules:

- Baselines are compared in Chromium only, with software WebGL — the configuration CI runs. Other engines antialias differently.
- A comparison tolerates a few dozen differing pixels (driver noise); a different glyph differs by far more.
- Re-record deliberately, then look at the images before committing them:

  ```bash
  E2E_BROWSERS=chromium pnpm --dir web exec playwright test --update-snapshots
  ```
