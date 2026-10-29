// Help › Tutorial: drafting's basics, then how to get the most from the AI,
// each step with something to try. A command runs as a menu's macro does;
// an AI example is put in the assistant's prompt, unsent, for the user to
// read and change. The words are data, in `tutorial.ts`.
import { Fragment, useEffect, useState, type ReactNode } from "react";
import type { Macro } from "../app/macro";
import { CHAPTERS, DRAWING, type TutorialChapter, type TutorialStep } from "./tutorial";

/** Text with `typed input` between backticks shown as code. */
function words(text: string): ReactNode {
  return text
    .split("`")
    .map((part, index) =>
      index % 2 === 1 ? <code key={index}>{part}</code> : <Fragment key={index}>{part}</Fragment>,
    );
}

function Step({
  step,
  onRun,
  onTry,
}: {
  step: TutorialStep;
  onRun: (macro: Macro) => void;
  onTry?: (prompt: string) => void;
}) {
  return (
    <section className="tutorial__step">
      <h3 className="tutorial__step-title">{step.title}</h3>
      {step.text.map((paragraph, index) => (
        <p key={index} className="tutorial__text">
          {words(paragraph)}
        </p>
      ))}
      {step.keys && (
        <table className="tutorial__keys">
          <tbody>
            {step.keys.map(([key, does]) => (
              <tr key={key}>
                <th scope="row">{words(key)}</th>
                <td>{does}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {step.command && (
        <button
          type="button"
          className="tutorial__try"
          onClick={() => step.command && onRun(step.command)}
        >
          Try {step.command.command}
        </button>
      )}
      {step.examples && (
        <ul className="tutorial__examples">
          {step.examples.map((example) => (
            <li key={example} className="tutorial__example">
              <span className="tutorial__example-text">{example}</span>
              {onTry && (
                <button
                  type="button"
                  className="tutorial__try"
                  aria-label={`Try: ${example}`}
                  onClick={() => onTry(example)}
                >
                  Try it
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function Tutorial({
  onRun,
  onTry,
  onClose,
}: {
  /** Runs a step's command; the tutorial closes. */
  onRun: (macro: Macro) => void;
  /** Puts an example in the AI's prompt; absent in a build without the AI. */
  onTry?: (prompt: string) => void;
  onClose: () => void;
}) {
  const [open, setOpen] = useState<TutorialChapter["id"]>("drawing");
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  const chapter = CHAPTERS.find((c) => c.id === open) ?? DRAWING;
  return (
    <div className="tutorial" role="dialog" aria-label="Tutorial" data-testid="tutorial">
      <div className="tutorial__header">
        <p className="tutorial__title">Tutorial</p>
        <div className="tutorial__tabs" role="tablist">
          {CHAPTERS.map((c) => (
            <button
              key={c.id}
              type="button"
              role="tab"
              aria-selected={c.id === open}
              className={`tutorial__tab${c.id === open ? " tutorial__tab--open" : ""}`}
              onClick={() => setOpen(c.id)}
            >
              {c.title}
            </button>
          ))}
        </div>
      </div>
      <div className="tutorial__body" role="tabpanel" aria-label={chapter.title}>
        <p className="tutorial__intro">{chapter.intro}</p>
        {chapter.steps.map((step) => (
          <Step key={step.title} step={step} onRun={onRun} onTry={onTry} />
        ))}
        {chapter.id === "ai" && !onTry && (
          <p className="tutorial__text tutorial__text--muted">
            This build has no AI service: the examples are here to read.
          </p>
        )}
      </div>
      <div className="warning__actions">
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          autoFocus
          onClick={onClose}
        >
          Close
        </button>
      </div>
    </div>
  );
}
