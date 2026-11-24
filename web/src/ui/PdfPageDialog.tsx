// The page of a PDF to attach as a picture (ADR-0042 §3, #528): the file's
// name, the page by its number — typed, or stepped through — with the page
// drawn small once pdf.js has drawn it, and Attach or Cancel. A narrow
// dialog over the canvas (§5), as the export warning is.
import { useEffect, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";

export function PdfPageDialog({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const choice = snapshot.pdfPages;
  useEffect(() => {
    if (!choice) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        app.cancelPdfPage();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [app, choice]);
  if (!choice) return null;
  return (
    <form
      className="warning warning--settings pdf-page"
      role="dialog"
      aria-label="Choose a page"
      data-testid="pdf-page"
      onSubmit={(event) => {
        event.preventDefault();
        app.choosePdfPage();
      }}
    >
      <p className="warning__title">Which page of {choice.name} do you want to attach?</p>
      <div className="pdf-page__body">
        <div className="pdf-page__thumbnail">
          {choice.thumbnail ? (
            <img src={choice.thumbnail} alt={`Page ${choice.page}`} />
          ) : (
            <span className="pdf-page__drawing">Drawing page {choice.page}…</span>
          )}
        </div>
        <div className="pdf-page__pager">
          <button
            type="button"
            className="text-editor__button"
            aria-label="Previous page"
            disabled={choice.page <= 1}
            onClick={() => app.showPdfPage(choice.page - 1)}
          >
            ‹
          </button>
          <label className="settings__label" htmlFor="pdf-page-number">
            Page
          </label>
          <input
            id="pdf-page-number"
            className="settings__field pdf-page__number"
            type="number"
            min={1}
            max={choice.pages}
            value={choice.page}
            autoFocus
            onChange={(event) => app.showPdfPage(Number(event.target.value))}
          />
          <span className="settings__label">of {choice.pages}</span>
          <button
            type="button"
            className="text-editor__button"
            aria-label="Next page"
            disabled={choice.page >= choice.pages}
            onClick={() => app.showPdfPage(choice.page + 1)}
          >
            ›
          </button>
        </div>
      </div>
      <p className="settings__note">
        The page becomes a picture under the drawing; its lines are not made into objects.
      </p>
      <div className="warning__actions">
        <button type="button" className="text-editor__button" onClick={() => app.cancelPdfPage()}>
          Cancel
        </button>
        <button type="submit" className="text-editor__button text-editor__button--primary">
          Attach
        </button>
      </div>
    </form>
  );
}
