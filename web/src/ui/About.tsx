// Help › About (#247): a narrow dialog (§5) with the core's version, what
// the program is, and what it does not do with a drawing.
import { useEffect } from "react";

export function About({ version, onClose }: { version: string; onClose: () => void }) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" && event.key !== "Enter") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  return (
    <div className="about" role="dialog" aria-label="About CadSlate2D">
      <p className="about__title">
        CadSlate<span className="header__brand-accent">2D</span>
      </p>
      <p className="about__line">
        Free 2D CAD in your browser. Draw it yourself, or describe it and the AI drafts it.
      </p>
      <p className="about__line" data-testid="about-version">
        Core version {version}
      </p>
      <p className="about__line about__line--muted">
        Free software under the GNU Affero General Public License, version 3 or later. Drawings stay
        in this browser and the files you choose: nothing is sent anywhere.
      </p>
      <p className="about__line about__line--muted">
        <a href="third-party-notices.txt" target="_blank" rel="noreferrer">
          Third-party notices
        </a>
      </p>
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
