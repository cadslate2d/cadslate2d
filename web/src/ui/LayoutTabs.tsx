// The tabs of the visual identity (#193): the model and each layout, the
// one the session is on marked; a click goes there through LAYOUT, a
// double-click renames through it, and "+" makes one. Rust owns the
// layouts; this only shows their names.
import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

export function LayoutTabs({ app }: { app: CadApp }) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const busy = ui.command !== undefined;
  const tab = (name: string, current: boolean, model: boolean) => (
    <button
      key={name}
      type="button"
      className={`status__chip status__tab${current ? " status__chip--active" : ""}`}
      aria-pressed={current}
      data-testid="layout-tab"
      title={model ? "Model space" : `Layout ${name}. Double-click to rename.`}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => {
        if (!busy && !current) app.run(model ? "MODEL" : "LAYOUT", ...(model ? [] : ["S", name]));
      }}
      onDoubleClick={() => {
        if (!busy && !model) app.run("LAYOUT", "R", name);
      }}
    >
      {name}
    </button>
  );
  const onModel = !ui.paper && !ui.through;
  return (
    <nav className="status__tabs" aria-label="Layouts">
      {tab("Model", onModel, true)}
      {ui.layouts.map((name) => tab(name, ui.space === name && !onModel, false))}
      <button
        type="button"
        className="status__chip status__tab"
        title="A new layout"
        aria-label="New layout"
        disabled={busy}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => app.run("LAYOUT", "N")}
      >
        +
      </button>
    </nav>
  );
}
