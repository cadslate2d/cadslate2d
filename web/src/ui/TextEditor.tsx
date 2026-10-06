// The paragraph editor (DESIGN §5; #144): a narrow, task-specific editor
// that opens while MTEXT — or TEXTEDIT on a paragraph — waits for words.
// It holds only the draft being typed; what it submits goes through the
// command line as text with its line breaks, like anything typed, and the
// command decides what becomes of it.
import { useState, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { KeyAction } from "../cad/engine";

export function TextEditor({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const initial = snapshot.textEditor;
  if (initial === undefined) return null;
  // Each request is a fresh draft: the log's length tells one from the next.
  return <Draft key={`${snapshot.log.length}:${initial}`} app={app} initial={initial} />;
}

function Draft({ app, initial }: { app: CadApp; initial: string }) {
  const [draft, setDraft] = useState(initial);
  const submit = () => app.submitText(draft);
  const cancel = () => app.key(KeyAction.Escape);

  return (
    <div className="text-editor" role="dialog" aria-label="Text editor">
      <textarea
        className="text-editor__field"
        aria-label="Paragraph"
        value={draft}
        rows={6}
        spellCheck={false}
        autoFocus
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          // Enter breaks the line; Ctrl+Enter (or ⌘+Enter) finishes.
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            submit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            cancel();
          }
          // Every other key is typing, not a command-line shortcut.
          event.stopPropagation();
        }}
      />
      <div className="text-editor__actions">
        <span className="text-editor__hint">Enter breaks the line; Ctrl+Enter finishes.</span>
        <button type="button" className="text-editor__button" onClick={cancel}>
          Cancel
        </button>
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          onClick={submit}
        >
          OK
        </button>
      </div>
    </div>
  );
}
