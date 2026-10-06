// The Layers panel (DESIGN §5, §7.2; #140): a second way to do what LAYER
// does, never the only one. It shows presentation data from the UI state
// and acts only by running LAYER with its answers — React holds no layer
// table and decides nothing about layers.
import { useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";
import type { CadApp } from "../app/cad-app";
import type { LayerRow } from "../cad/engine";
import {
  createLayer,
  deleteLayer,
  renameLayer,
  setCurrent,
  swatchColor,
  toggleLocked,
  toggleOn,
  type CommandScript,
} from "./layer-actions";

const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;
const icon = (children: ReactNode) => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" {...stroke}>
    {children}
  </svg>
);
const EYE = icon(
  <>
    <path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z" />
    <circle cx="8" cy="8" r="2" />
  </>,
);
const EYE_OFF = icon(
  <path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8zM3 13 13 3" />,
);
const LOCK = icon(
  <>
    <rect x="3.5" y="7" width="9" height="6.5" rx="1" />
    <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" />
  </>,
);
const UNLOCK = icon(
  <>
    <rect x="3.5" y="7" width="9" height="6.5" rx="1" />
    <path d="M5.5 7V5a2.5 2.5 0 0 1 5-.5" />
  </>,
);
const PENCIL = icon(<path d="M3 13h2.5L13 5.5 10.5 3 3 10.5zM9 4.5 11.5 7" />);
const CROSS = icon(<path d="M4 4l8 8M12 4l-8 8" />);

/** A one-line text field that submits on Enter and gives up on Escape. */
function NameField({
  label,
  initial,
  onSubmit,
  onCancel,
}: {
  label: string;
  initial: string;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = new FormData(event.currentTarget).get("name");
    if (typeof name === "string" && name.trim()) onSubmit(name.trim());
    else onCancel();
  };
  return (
    <form className="layers__form" onSubmit={submit}>
      <input
        className="layers__field"
        name="name"
        aria-label={label}
        defaultValue={initial}
        autoFocus
        autoComplete="off"
        onKeyDown={(event) => {
          if (event.key === "Escape") onCancel();
          // Keep every other key — Space included — in the field.
          event.stopPropagation();
        }}
        onBlur={onCancel}
      />
    </form>
  );
}

function Row({
  layer,
  busy,
  run,
}: {
  layer: LayerRow;
  busy: boolean;
  /** Runs a script unless the panel is busy. */
  run: (script: CommandScript) => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const button = (
    label: string,
    pressed: boolean | undefined,
    script: CommandScript | undefined,
    children: ReactNode,
    extra = "",
  ) => (
    <button
      type="button"
      className={`layers__button${extra}`}
      aria-label={`${label} ${layer.name}`}
      aria-pressed={pressed}
      title={label}
      disabled={busy || !script}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => script && run(script)}
    >
      {children}
    </button>
  );
  return (
    <tr
      className={`layers__row${layer.current ? " layers__row--current" : ""}`}
      aria-current={layer.current || undefined}
    >
      <td>
        {button(
          layer.on ? "Switch off" : "Switch on",
          layer.on,
          toggleOn(layer),
          layer.on ? EYE : EYE_OFF,
        )}
      </td>
      <td>
        {button(
          layer.locked ? "Unlock" : "Lock",
          layer.locked,
          toggleLocked(layer),
          layer.locked ? LOCK : UNLOCK,
        )}
      </td>
      <td className="layers__name">
        {renaming ? (
          <NameField
            label={`New name for ${layer.name}`}
            initial={layer.name}
            onSubmit={(name) => {
              setRenaming(false);
              if (name !== layer.name) run(renameLayer(layer, name));
            }}
            onCancel={() => setRenaming(false)}
          />
        ) : (
          <button
            type="button"
            className="layers__button layers__button--name"
            aria-label={`Make ${layer.name} current`}
            title="Make current"
            disabled={busy || layer.current}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => run(setCurrent(layer))}
          >
            {layer.name}
          </button>
        )}
      </td>
      <td>
        <span
          className="layers__swatch"
          style={{ background: swatchColor(layer) }}
          aria-hidden="true"
        />
        <span className="layers__muted">{layer.color}</span>
      </td>
      <td className="layers__muted">{layer.linetype}</td>
      <td className="layers__muted">{layer.lineweight}</td>
      <td className="layers__tools">
        <button
          type="button"
          className="layers__button"
          aria-label={`Rename ${layer.name}`}
          title="Rename"
          disabled={busy || layer.name === "0"}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setRenaming(true)}
        >
          {PENCIL}
        </button>
        {button(
          "Delete",
          undefined,
          layer.name === "0" ? undefined : deleteLayer(layer),
          CROSS,
          " layers__button--danger",
        )}
      </td>
    </tr>
  );
}

export function LayersPanel({ app }: { app: CadApp }) {
  const snapshot = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const [creating, setCreating] = useState(false);
  // A command in progress has the command line: the panel waits for it.
  const busy = snapshot.command !== undefined;
  const run = (script: CommandScript) => {
    if (!busy) app.run(...script);
  };

  return (
    <aside className="layers" aria-label="Layers">
      <div className="layers__header">
        <h2 className="layers__title">Layers</h2>
        <button
          type="button"
          className="layers__button"
          aria-label="New layer"
          title="New layer"
          disabled={busy}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setCreating(true)}
        >
          {icon(<path d="M8 3v10M3 8h10" />)}
        </button>
      </div>
      {creating && (
        <NameField
          label="Name of the new layer"
          initial=""
          onSubmit={(name) => {
            setCreating(false);
            run(createLayer(name));
          }}
          onCancel={() => setCreating(false)}
        />
      )}
      <div className="layers__scroll">
        <table className="layers__table">
          <thead>
            <tr>
              <th scope="col" title="On">
                On
              </th>
              <th scope="col" title="Locked">
                Lock
              </th>
              <th scope="col">Name</th>
              <th scope="col">Color</th>
              <th scope="col">Linetype</th>
              <th scope="col">Lineweight</th>
              <th scope="col">
                <span className="visually-hidden">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {snapshot.layers.map((layer) => (
              <Row key={layer.name} layer={layer} busy={busy} run={run} />
            ))}
          </tbody>
        </table>
      </div>
      {busy && <p className="layers__note">Finish the current command to change layers.</p>}
    </aside>
  );
}
