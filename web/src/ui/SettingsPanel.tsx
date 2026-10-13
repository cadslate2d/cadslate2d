// The settings panel (#186): what the browser keeps between sessions —
// the theme, the autosave delay, the drawing unit, the drafting toggles,
// and the user's aliases. The toggles and the unit are the engine's and
// change through the command line, as everything in the drawing does; the
// theme and the delay are the shell's. Nothing here leaves the browser.
import { useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import type { Theme } from "../app/host-services";
import { TOGGLES } from "../app/toggles";

/** The units UNITS knows, as its keywords and the suffix the state reports. */
const UNITS: readonly { keyword: string; suffix: string; label: string }[] = [
  { keyword: "Unitless", suffix: "", label: "Unitless" },
  { keyword: "MM", suffix: "mm", label: "Millimetres" },
  { keyword: "CM", suffix: "cm", label: "Centimetres" },
  { keyword: "M", suffix: "m", label: "Metres" },
  { keyword: "IN", suffix: "in", label: "Inches" },
  { keyword: "FT", suffix: "ft", label: "Feet" },
];

export function SettingsPanel({ app }: { app: CadApp }) {
  const settings = useSyncExternalStore(app.settings.subscribe, app.settings.getSnapshot);
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  if (!settings.open) return null;
  const busy = ui.command !== undefined;
  const setUnit = (keyword: string) => {
    if (!busy) app.run("UNITS", keyword);
  };
  return (
    <div className="warning warning--settings" role="dialog" aria-label="Settings">
      <p className="warning__title">Settings</p>
      <p className="settings__note">
        Kept in this browser, for this site; nothing leaves it. The toggles come back as last left.
      </p>
      <div className="settings__grid">
        <label className="settings__label" htmlFor="settings-theme">
          Theme
        </label>
        <select
          id="settings-theme"
          className="settings__field"
          value={settings.theme}
          onChange={(event) => app.setTheme(event.target.value as Theme)}
        >
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>

        <label className="settings__label" htmlFor="settings-autosave">
          Autosave after (seconds)
        </label>
        <input
          id="settings-autosave"
          className="settings__field"
          type="number"
          min={0.1}
          step={0.5}
          value={settings.autosaveDelayMs / 1000}
          onChange={(event) => {
            const seconds = Number(event.target.value);
            if (Number.isFinite(seconds)) app.setAutosaveDelay(seconds * 1000);
          }}
        />

        <label className="settings__label" htmlFor="settings-unit">
          Drawing unit
        </label>
        <select
          id="settings-unit"
          className="settings__field"
          value={ui.unit}
          disabled={busy}
          title={busy ? "Finish the command first" : "Sets the unit with the UNITS command"}
          onChange={(event) =>
            setUnit(UNITS.find((unit) => unit.suffix === event.target.value)?.keyword ?? "Unitless")
          }
        >
          {UNITS.map((unit) => (
            <option key={unit.suffix} value={unit.suffix}>
              {unit.label}
            </option>
          ))}
        </select>

        <span className="settings__label">Drafting toggles</span>
        <div className="settings__toggles">
          {TOGGLES.map((toggle) => {
            const on = (ui.toggles & toggle.bit) !== 0;
            return (
              <label key={toggle.id} className="settings__toggle" title={toggle.hint}>
                <input type="checkbox" checked={on} onChange={() => app.key(toggle.action)} />
                {toggle.label}
              </label>
            );
          })}
        </div>

        <span className="settings__label">Aliases</span>
        <div className="settings__aliases">
          <pre className="settings__alias-list" data-testid="alias-list">
            {ui.userAliases === "" ? "None yet." : ui.userAliases}
          </pre>
          <button
            type="button"
            className="text-editor__button"
            disabled={busy}
            onClick={() => {
              app.closeSettings();
              app.run("ALIASEDIT");
            }}
          >
            Edit aliases…
          </button>
        </div>
      </div>
      <div className="warning__actions">
        <button
          type="button"
          className="text-editor__button text-editor__button--primary"
          autoFocus
          onClick={() => app.closeSettings()}
        >
          Close
        </button>
      </div>
    </div>
  );
}
