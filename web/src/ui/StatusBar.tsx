import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { STATUS_BAR } from "../app/toggles";

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
      {ui.exportLosses.length > 0 && (
        <span
          className="status__chip status__lossy"
          data-testid="lossy"
          title={`An export will not include everything:\n${ui.exportLosses.join("\n")}`}
        >
          LOSSY
        </span>
      )}
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
      {STATUS_BAR.map((toggle) => {
        if (typeof toggle === "string") {
          return (
            <span
              key={toggle}
              className="status__chip status__chip--unavailable"
              aria-disabled="true"
              title={`${toggle} is not available yet`}
            >
              {toggle}
            </span>
          );
        }
        const on = (ui.toggles & toggle.bit) !== 0;
        const key = toggle.key ? ` (${toggle.key})` : "";
        return (
          <button
            key={toggle.id}
            type="button"
            className={`status__chip status__toggle${on ? " status__chip--active" : ""}`}
            aria-pressed={on}
            title={`${toggle.label}${key}. ${toggle.hint}`}
            // Keep keyboard focus on the command line.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => app.key(toggle.action)}
          >
            {toggle.label}
          </button>
        );
      })}
      <span className="status__chip status__chip--active">MODEL</span>
    </footer>
  );
}
