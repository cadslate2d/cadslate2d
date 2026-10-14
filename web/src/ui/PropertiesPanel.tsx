// The Properties panel (DESIGN §5, §32; #141): what is selected, the four
// properties it shares, and — for one object — its defining geometry. The
// summary is computed in Rust and bounded; a change runs CHPROP through the
// command line. React holds nothing about the objects themselves.
import { useState, useSyncExternalStore, type FormEvent } from "react";
import type { CadApp } from "../app/cad-app";
import type { UiSnapshot } from "../app/ui-store";
import {
  CHANGEABLE,
  changeProperty,
  COLOR_CHOICES,
  LINEWEIGHT_CHOICES,
  type Changeable,
} from "./layer-actions";

/** Shown in a field whose objects do not agree. */
const VARIES = "*Varies*";

/** What a property field offers, from the drawing's tables or the fixed lists. */
function choicesFor(property: Changeable, snapshot: UiSnapshot): readonly string[] {
  switch (property) {
    case "Layer":
      return snapshot.layers.map((layer) => layer.name);
    case "Color":
      return COLOR_CHOICES;
    case "Linetype":
      return ["ByLayer", "ByBlock", ...snapshot.linetypes];
    case "Lineweight":
      return LINEWEIGHT_CHOICES;
  }
}

function valueOf(property: Changeable, snapshot: UiSnapshot): string | undefined {
  const { selection } = snapshot;
  switch (property) {
    case "Layer":
      return selection.layer;
    case "Color":
      return selection.color;
    case "Linetype":
      return selection.linetype;
    case "Lineweight":
      return selection.lineweight;
  }
}

/**
 * One property: its shared value, editable as text with the table's names
 * offered. Submitting runs CHPROP; the field shows whatever Rust decided.
 */
function PropertyField({
  property,
  snapshot,
  busy,
  onChange,
}: {
  property: Changeable;
  snapshot: UiSnapshot;
  busy: boolean;
  onChange: (property: Changeable, value: string) => void;
}) {
  const value = valueOf(property, snapshot);
  const [editing, setEditing] = useState(false);
  const id = `property-${property.toLowerCase()}`;
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const typed = new FormData(event.currentTarget).get("value");
    setEditing(false);
    if (typeof typed === "string" && typed.trim() && typed.trim() !== value) {
      onChange(property, typed.trim());
    }
  };
  return (
    <tr>
      <th scope="row">
        <label htmlFor={id}>{property}</label>
      </th>
      <td>
        {editing ? (
          <form className="properties__form" onSubmit={submit}>
            <input
              id={id}
              className="properties__field"
              name="value"
              list={`${id}-choices`}
              defaultValue={value ?? ""}
              autoFocus
              autoComplete="off"
              onKeyDown={(event) => {
                if (event.key === "Escape") setEditing(false);
                event.stopPropagation();
              }}
              onBlur={() => setEditing(false)}
            />
            <datalist id={`${id}-choices`}>
              {choicesFor(property, snapshot).map((choice) => (
                <option key={choice} value={choice} />
              ))}
            </datalist>
          </form>
        ) : (
          <button
            id={id}
            type="button"
            className={`properties__value${value === undefined ? " properties__value--varies" : ""}`}
            title={`Change ${property.toLowerCase()}`}
            disabled={busy}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setEditing(true)}
          >
            {value ?? VARIES}
          </button>
        )}
      </td>
    </tr>
  );
}

export function PropertiesPanel({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const { selection } = snapshot;
  // A command in progress has the command line: the panel waits for it.
  const busy = snapshot.command !== undefined;
  const change = (property: Changeable, value: string) => {
    if (!busy) app.run(...changeProperty(property, value));
  };

  return (
    <aside className="properties" aria-label="Properties">
      <div className="properties__header">
        <h2 className="properties__title">Properties</h2>
        <span className="properties__headline" data-testid="selection-headline">
          {selection.headline}
        </span>
        {selection.method !== "" && (
          <span className="properties__method" data-testid="selection-method">
            by {selection.method}
          </span>
        )}
        {selection.groups.length > 0 && (
          <span className="properties__method" data-testid="selection-groups">
            in {selection.groups.length === 1 ? "group" : "groups"} {selection.groups.join(", ")}
          </span>
        )}
      </div>
      {selection.count > 0 && (
        <div className="properties__scroll">
          <table className="properties__table">
            <tbody>
              <tr className="properties__group">
                <th scope="rowgroup" colSpan={2}>
                  General
                </th>
              </tr>
              {CHANGEABLE.map(({ label }) => (
                <PropertyField
                  key={label}
                  property={label}
                  snapshot={snapshot}
                  busy={busy}
                  onChange={change}
                />
              ))}
              {selection.geometry.length > 0 && (
                <>
                  <tr className="properties__group">
                    <th scope="rowgroup" colSpan={2}>
                      Geometry
                    </th>
                  </tr>
                  {selection.geometry.map(([label, value]) => (
                    <tr key={label}>
                      <th scope="row">{label}</th>
                      <td className="properties__readonly">{value}</td>
                    </tr>
                  ))}
                </>
              )}
            </tbody>
          </table>
        </div>
      )}
      {busy && selection.count > 0 && (
        <p className="properties__note">Finish the current command to change properties.</p>
      )}
    </aside>
  );
}
