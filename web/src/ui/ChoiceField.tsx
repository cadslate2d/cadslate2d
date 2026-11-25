// A value that is changed by choosing from a list or typing: the current
// value as a button, and on a click a field offering every choice. The
// field opens empty, the current value its placeholder: a browser offers
// only the choices that match what is in the field, so a field opened with
// "ByLayer" in it offered "ByLayer" alone and nothing could be chosen.
import { useState, type FormEvent } from "react";

export function ChoiceField({
  id,
  label,
  value,
  shown = value,
  choices,
  disabled,
  className,
  onChange,
}: {
  id: string;
  /** The accessible name of the button and of the field. */
  label: string;
  /** The current value; undefined where the objects do not agree. */
  value: string | undefined;
  /** What the button shows, when not the value itself. */
  shown?: string;
  choices: readonly string[];
  disabled: boolean;
  className: string;
  /** A value chosen or typed that differs from the current one. */
  onChange: (value: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const typed = new FormData(event.currentTarget).get("value");
    setEditing(false);
    if (typeof typed === "string" && typed.trim() && typed.trim() !== value) {
      onChange(typed.trim());
    }
  };
  if (editing) {
    return (
      <form className="choice__form" onSubmit={submit}>
        <input
          id={id}
          className="choice__field"
          name="value"
          aria-label={label}
          list={`${id}-choices`}
          placeholder={shown}
          autoFocus
          autoComplete="off"
          onKeyDown={(event) => {
            if (event.key === "Escape") setEditing(false);
            event.stopPropagation();
          }}
          onBlur={() => setEditing(false)}
        />
        <datalist id={`${id}-choices`}>
          {choices.map((choice) => (
            <option key={choice} value={choice} />
          ))}
        </datalist>
      </form>
    );
  }
  return (
    <button
      id={id}
      type="button"
      className={className}
      aria-label={label}
      title={`Change ${label.toLowerCase()}`}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => setEditing(true)}
    >
      {shown}
    </button>
  );
}
