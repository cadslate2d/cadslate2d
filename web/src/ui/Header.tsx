import type { ReactNode } from "react";

interface HeaderProps {
  coreVersion: string;
  /** At the row's right: the AI account, when signed in. */
  account?: ReactNode;
}

/** The brand row. What the drafter acts on is the menu bar beneath it (DESIGN §5). */
export function Header({ coreVersion, account }: HeaderProps) {
  return (
    <header className="header" data-core-version={coreVersion}>
      <h1 className="header__brand">
        CadSlate<span className="header__brand-accent">2D</span>
      </h1>
      <span className="header__tagline">Classic 2D CAD. In your browser.</span>
      {account && <div className="header__end">{account}</div>}
    </header>
  );
}
