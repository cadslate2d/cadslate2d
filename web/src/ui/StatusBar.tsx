/** Drafting toggles from DESIGN §5. They gain behaviour in M1–M2. */
const TOGGLES = ["SNAP", "GRID", "ORTHO", "POLAR", "OSNAP", "OTRACK", "LWT"] as const;

export function StatusBar() {
  return (
    <footer className="status" aria-label="Status bar">
      <span className="status__chip status__chip--active">Model</span>
      <span className="status__spacer" />
      <span className="status__chip status__coordinates" data-testid="coordinates">
        —
      </span>
      {TOGGLES.map((toggle) => (
        <span
          key={toggle}
          className="status__chip status__chip--unavailable"
          aria-disabled="true"
          title={`${toggle} is not available yet`}
        >
          {toggle}
        </span>
      ))}
      <span className="status__chip status__chip--active">MODEL</span>
    </footer>
  );
}
