import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { TOGGLES, UNAVAILABLE_TOGGLES } from "../app/toggles";

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
      {ui.tracking && (
        <span className="status__chip status__tracking" data-testid="tracking">
          {ui.tracking}
        </span>
      )}
      {ui.snap && (
        <span className="status__chip status__snap" data-testid="snap">
          {ui.snap}
        </span>
      )}
      <span className="status__chip status__coordinates" data-testid="coordinates">
        {formatCoordinates(ui.cursor)}
      </span>
      {TOGGLES.map((toggle) => {
        const on = (ui.toggles & toggle.bit) !== 0;
        return (
          <button
            key={toggle.id}
            type="button"
            className={`status__chip status__toggle${on ? " status__chip--active" : ""}`}
            aria-pressed={on}
            title={`${toggle.label} (${toggle.key}). ${toggle.hint}`}
            // Keep keyboard focus on the command line.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => app.key(toggle.action)}
          >
            {toggle.label}
          </button>
        );
      })}
      {UNAVAILABLE_TOGGLES.map((label) => (
        <span
          key={label}
          className="status__chip status__chip--unavailable"
          aria-disabled="true"
          title={`${label} is not available yet`}
        >
          {label}
        </span>
      ))}
      <span className="status__chip status__chip--active">MODEL</span>
    </footer>
  );
}
