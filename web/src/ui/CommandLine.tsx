import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { classifyKey } from "../app/key-routing";
import { KeyAction } from "../cad/engine";

/** Number of rows in the command area; must match --command-visible-lines. */
const VISIBLE_LINES = 5;

/**
 * The command line: recent log lines, the current prompt, and a native text
 * input. It owns only the text being typed. On submit the raw string goes to
 * Rust, which decides what it means (ADR-0002 §6).
 */
export function CommandLine({ app }: { app: CadApp }) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Keyboard is a first-class input (DESIGN §13.4): keys pressed anywhere on
  // the page drive the command line, without clicking it first.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const typed = inputRef.current?.value ?? "";
      const intent = classifyKey(event, app.ui.getSnapshot().spaceAccepts, typed === "");
      switch (intent.kind) {
        case "ignore":
          return;
        case "text":
          // Let the keystroke land in the input, wherever focus was.
          inputRef.current?.focus();
          return;
        case "submit":
          app.submitText(inputRef.current?.value ?? "");
          setText("");
          break;
        case "escape":
          setText("");
          app.key(KeyAction.Escape);
          break;
        case "undo":
          app.key(KeyAction.Undo);
          break;
        case "redo":
          app.key(KeyAction.Redo);
          break;
        case "open":
          app.key(KeyAction.Open);
          break;
        case "save":
          app.key(KeyAction.Save);
          break;
        case "delete":
          app.key(KeyAction.Delete);
          break;
        case "history-previous":
        case "history-next": {
          const recalled =
            intent.kind === "history-previous" ? app.historyPrevious() : app.historyNext();
          if (recalled !== undefined) setText(recalled);
          break;
        }
      }
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [app]);

  const history = ui.log.slice(-(VISIBLE_LINES - 1));
  const firstLineNumber = ui.log.length - history.length + 1;

  return (
    <section className="command" aria-label="Command line">
      <ol className="command__gutter" aria-hidden="true">
        {Array.from({ length: history.length + 1 }, (_, index) => (
          <li key={index}>{firstLineNumber + index}</li>
        ))}
      </ol>
      <ol className="command__lines" aria-live="polite">
        {history.map((line, index) => (
          <li key={firstLineNumber + index} data-testid="log-line">
            {line}
          </li>
        ))}
        <li className="command__line--prompt">
          <label htmlFor="command-input" data-testid="prompt">
            {ui.prompt}
          </label>
          <input
            id="command-input"
            ref={inputRef}
            className="command__input"
            value={text}
            onChange={(event) => setText(event.target.value)}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            autoFocus
          />
        </li>
      </ol>
    </section>
  );
}
