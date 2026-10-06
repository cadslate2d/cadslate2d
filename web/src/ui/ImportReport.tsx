// The import report (DESIGN §24.2; #157): what the last OPEN brought in and
// what it did not, kept on screen until dismissed or another drawing is
// opened. The lines are the engine's; this shows them and nothing else.
import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

export function ImportReport({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const report = snapshot.importReport;
  if (!report) return null;
  const lossy = snapshot.exportLosses.length > 0;
  return (
    <aside
      className={`report${lossy ? " report--lossy" : ""}`}
      role="status"
      aria-label="Import report"
      data-testid="import-report"
    >
      <div className="report__header">
        <span className="report__title">
          {snapshot.documentName ?? "Import"}
          {lossy && <span className="report__badge">not all of it</span>}
        </span>
        <button
          type="button"
          className="report__dismiss"
          aria-label="Dismiss the import report"
          // Keep keyboard focus on the command line.
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => app.dismissImportReport()}
        >
          ×
        </button>
      </div>
      <ul className="report__lines">
        {report.lines.map((line, index) => (
          <li
            key={`${index}:${line}`}
            className={snapshot.exportLosses.includes(line) ? "report__line--loss" : undefined}
          >
            {line}
          </li>
        ))}
      </ul>
    </aside>
  );
}
