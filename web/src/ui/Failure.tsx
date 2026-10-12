// The engine has stopped (#176; §5: narrow and task-specific): a Rust
// panic aborts the session, and nothing can reach it afterwards. The dialog
// says what happened, that the drawing as last kept is safe (ADR-0019),
// and offers the reload that brings it back.
import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

/** "The drawing as kept at 14:05 is safe …" or what to say when nothing was kept. */
export function describeKept(keptAt: number | undefined): string {
  if (keptAt === undefined) {
    return "Nothing had been kept since the last file was opened or saved; that file is as it was.";
  }
  const time = new Date(keptAt).toLocaleString();
  return `The drawing as kept at ${time} is safe: reloading offers it back.`;
}

export function Failure({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const failure = snapshot.failure;
  if (!failure) return null;
  return (
    <div
      className="warning warning--failure"
      role="alertdialog"
      aria-label="The drawing engine stopped"
      data-testid="failure"
    >
      <p className="warning__title">The drawing engine stopped and cannot continue.</p>
      <ul className="warning__lines">
        <li>{failure.message}</li>
        <li>{describeKept(failure.keptAt)}</li>
      </ul>
      <div className="warning__actions">
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          autoFocus
          onClick={() => app.reload()}
        >
          Reload
        </button>
      </div>
    </div>
  );
}
