import type { Visibility } from "./App";

interface HeaderProps {
  coreVersion: string;
  /** Present once the app is ready: whether the toolbar is shown, and how to flip it. */
  toolbar?: Visibility;
  /** Likewise for the Layers panel. */
  layers?: Visibility;
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

export function Header({ coreVersion, toolbar, layers }: HeaderProps) {
  return (
    <header className="header" data-core-version={coreVersion}>
      <h1 className="header__brand">
        CadSlate<span className="header__brand-accent">2D</span>
      </h1>
      <span className="header__tagline">Classic 2D CAD. In your browser.</span>
      <span className="header__spacer" />
      {toolbar && <Switch label="Toolbar" part={toolbar} />}
      {layers && <Switch label="Layers" part={layers} />}
    </header>
  );
}
