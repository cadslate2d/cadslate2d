import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { classifyKey } from "../app/key-routing";
import { toggleSpec } from "../app/toggles";
import { KeyAction } from "../cad/engine";

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
      // Words selected on the page — a reply, a line of the log — are what
      // Ctrl+C copies, not the drawing's selection.
      const selected = (window.getSelection()?.toString() ?? "") !== "";
      const intent = classifyKey(event, app.ui.getSnapshot().spaceAccepts, typed === "", selected);
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
        case "group-selection":
          app.key(KeyAction.GroupSelection);
          break;
        case "isoplane":
          app.key(KeyAction.Isoplane);
          break;
        case "command":
          setText("");
          app.invoke(intent.command);
          break;
        case "select-all":
          // At the idle prompt only: in a command, everything is the
          // command's `ALL` keyword to type.
          if (app.ui.getSnapshot().command === undefined) app.invoke("ALL");
          break;
        case "copy":
          app.key(KeyAction.Copy);
          break;
        case "cut":
          app.key(KeyAction.Cut);
          break;
        case "paste":
          app.key(KeyAction.Paste);
          break;
        case "delete":
          app.key(KeyAction.Delete);
          break;
        case "toggle":
          app.key(toggleSpec(intent.toggle).action);
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

  // Keep the prompt in view as lines arrive; the user can scroll back at will.
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const scroller = scrollRef.current;
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  }, [ui.log.length, ui.prompt]);

  // Line numbers count from the start of the session, even after old lines scroll out of the store.
  const promptNumber = app.ui.logTotal + 1;
  const firstNumber = promptNumber - ui.log.length;

  return (
    <section className="command" aria-label="Command line">
      <div className="command__scroll" ref={scrollRef} data-testid="command-scroll">
        <ol className="command__rows" aria-live="polite">
          {ui.log.map((line, index) => (
            <li key={firstNumber + index} className="command__row">
              <span className="command__number" aria-hidden="true">
                {firstNumber + index}
              </span>
              <span data-testid="log-line">{line}</span>
            </li>
          ))}
          <li className="command__row command__row--prompt">
            <span className="command__number" aria-hidden="true">
              {promptNumber}
            </span>
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
      </div>
    </section>
  );
}
