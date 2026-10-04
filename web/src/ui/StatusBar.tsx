import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

/** Drafting toggles from DESIGN §5. They gain behaviour in M1–M2. */
const TOGGLES = ["SNAP", "GRID", "ORTHO", "POLAR", "OSNAP", "OTRACK", "LWT"] as const;
const COORDINATE_DECIMALS = 4;

export function formatCoordinates(cursor: readonly [number, number] | undefined): string {
  if (!cursor) return "—";
  return cursor.map((value) => value.toFixed(COORDINATE_DECIMALS)).join(", ");
}

export function StatusBar({ app }: { app: CadApp }) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  return (
    <footer className="status" aria-label="Status bar">
      <span className="status__chip status__chip--active">Model</span>
      <span className="status__spacer" />
      <span className="status__chip status__coordinates" data-testid="coordinates">
        {formatCoordinates(ui.cursor)}
      </span>
      {TOGGLES.map((toggle) => (
        <span
          key={toggle}
          className="status__chip status__chip--unavailable"
          aria-disabled="true"
          title={`${toggle} is not available yet`}
        >
          {toggle}
        </span>
      ))}
      <span className="status__chip status__chip--active">MODEL</span>
    </footer>
  );
}
