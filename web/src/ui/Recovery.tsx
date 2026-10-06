// The recovery offer (DESIGN §25; ADR-0019; §5: narrow and task-specific):
// a drawing kept in the browser that no file holds — a tab closed, a
// browser crashed — offered back once, on start. Recovering is a normal open.
import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

/** "plan.dxf, kept at 14:05" or "An unnamed drawing, kept at 14:05". */
export function describeRecovery(name: string | undefined, savedAt: number): string {
  const time = new Date(savedAt).toLocaleString();
  return `${name ?? "An unnamed drawing"}, kept at ${time}`;
}

export function Recovery({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const recovery = snapshot.recovery;
  if (!recovery) return null;
  return (
    <div
      className="warning warning--recovery"
      role="dialog"
      aria-label="Recover a drawing"
      data-testid="recovery"
    >
      <p className="warning__title">A drawing was not exported before this page was last left.</p>
      <ul className="warning__lines">
        <li>{describeRecovery(recovery.name, recovery.savedAt)}</li>
      </ul>
      <div className="warning__actions">
        <button type="button" className="text-editor__button" onClick={() => app.discardRecovery()}>
          Discard
        </button>
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          autoFocus
          onClick={() => app.recover()}
        >
          Recover
        </button>
      </div>
    </div>
  );
}
