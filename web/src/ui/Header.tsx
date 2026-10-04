export function Header({ coreVersion }: { coreVersion: string }) {
  return (
    <header className="header" data-core-version={coreVersion}>
      <h1 className="header__brand">
        CadSlate<span className="header__brand-accent">2D</span>
      </h1>
      <span className="header__tagline">Classic 2D CAD. In your browser.</span>
    </header>
  );
}
