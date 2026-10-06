// The warning before a lossy export (DESIGN §24.2, LOCKED; §5: narrow and
// task-specific): the file will not include what the import could not
// hold, named line by line, and the user exports anyway or does not.
import { useEffect, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

export function ExportWarning({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const losses = snapshot.exportWarning;
  useEffect(() => {
    if (!losses) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        app.cancelExport();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [app, losses]);
  if (!losses) return null;
  return (
    <div className="warning" role="dialog" aria-label="Export warning" data-testid="export-warning">
      <p className="warning__title">The file will not include everything this drawing came from.</p>
      <ul className="warning__lines">
        {losses.map((line, index) => (
          <li key={`${index}:${line}`}>{line}</li>
        ))}
      </ul>
      <div className="warning__actions">
        <button
          type="button"
          className="text-editor__button"
          autoFocus
          onClick={() => app.cancelExport()}
        >
          Cancel
        </button>
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          onClick={() => app.exportAnyway()}
        >
          Export anyway
        </button>
      </div>
    </div>
  );
}
