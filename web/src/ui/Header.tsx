import type { Visibility } from "./App";

interface HeaderProps {
  coreVersion: string;
  /** Opens the settings panel through the SETTINGS command, once the app is ready. */
  settings?: () => void;
  /** Present once the app is ready: whether the toolbar is shown, and how to flip it. */
  toolbar?: Visibility;
  /** Likewise for the Layers panel and the Properties panel. */
  layers?: Visibility;
  properties?: Visibility;
}

function Switch({ label, part }: { label: string; part: Visibility }) {
  return (
    <button
      type="button"
      className="header__button"
      aria-pressed={part.visible}
      onMouseDown={(event) => event.preventDefault()}
      onClick={part.toggle}
    >
      {label}
    </button>
  );
}

export function Header({ coreVersion, toolbar, layers, properties, settings }: HeaderProps) {
  return (
    <header className="header" data-core-version={coreVersion}>
      <h1 className="header__brand">
        CadSlate<span className="header__brand-accent">2D</span>
      </h1>
      <span className="header__tagline">Classic 2D CAD. In your browser.</span>
      <span className="header__spacer" />
      {toolbar && <Switch label="Toolbar" part={toolbar} />}
      {layers && <Switch label="Layers" part={layers} />}
      {properties && <Switch label="Properties" part={properties} />}
      {settings && (
        <button
          type="button"
          className="header__button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={settings}
        >
          Settings
        </button>
      )}
    </header>
  );
}
