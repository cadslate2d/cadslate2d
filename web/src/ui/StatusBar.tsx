import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { STATUS_BAR } from "../app/toggles";
import { KeyAction } from "../cad/engine";
import { LayoutTabs } from "./LayoutTabs";

const COORDINATE_DECIMALS = 4;

export function formatCoordinates(cursor: readonly [number, number] | undefined): string {
  if (!cursor) return "—";
  return cursor.map((value) => value.toFixed(COORDINATE_DECIMALS)).join(", ");
}

export function StatusBar({ app }: { app: CadApp }) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  return (
    <footer className="status" aria-label="Status bar">
      <LayoutTabs app={app} />
      <span
        className="status__chip status__document"
        data-testid="document"
        title={ui.unsavedChanges ? "Changed since it was last saved" : "Saved"}
      >
        {ui.documentName ?? "Untitled"}
        {ui.unsavedChanges ? " •" : ""}
      </span>
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
      {ui.isoplane && (
        <button
          type="button"
          className="status__chip status__toggle status__chip--active"
          data-testid="isoplane"
          title="The isometric plane ORTHO and isocircles work on (F5). Click for the next; type SNAP Style to leave isometric drafting."
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => app.key(KeyAction.Isoplane)}
        >
          ISO {ui.isoplane.toUpperCase()}
        </button>
      )}
      <button
        type="button"
        className="status__chip status__toggle status__chip--active"
        data-testid="space"
        title={
          ui.through
            ? "The model, worked through a viewport of the layout. Click for PSPACE."
            : ui.paper
              ? "On the layout's paper. Click for MSPACE, the model through a viewport."
              : "The model."
        }
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => {
          if (ui.through) app.run("PSPACE");
          else if (ui.paper) app.run("MSPACE");
        }}
      >
        {ui.paper ? "PAPER" : "MODEL"}
      </button>
    </footer>
  );
}
