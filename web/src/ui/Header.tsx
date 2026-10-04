interface HeaderProps {
  coreVersion: string;
  /** Present once the app is ready: whether the toolbar is shown, and how to flip it. */
  toolbar?: { visible: boolean; toggle: () => void };
}

export function Header({ coreVersion, toolbar }: HeaderProps) {
  return (
    <header className="header" data-core-version={coreVersion}>
      <h1 className="header__brand">
        CadSlate<span className="header__brand-accent">2D</span>
      </h1>
      <span className="header__tagline">Classic 2D CAD. In your browser.</span>
      <span className="header__spacer" />
      {toolbar && (
        <button
          type="button"
          className="header__button"
          aria-pressed={toolbar.visible}
          onMouseDown={(event) => event.preventDefault()}
          onClick={toolbar.toggle}
        >
          Toolbar
        </button>
      )}
    </header>
  );
}
