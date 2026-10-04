/** Number of rows in the command area; must match --command-visible-lines. */
const VISIBLE_LINES = 5;

export function CommandLine() {
  const lines = Array.from({ length: VISIBLE_LINES }, (_, index) => index + 1);
  return (
    <section className="command" aria-label="Command line">
      <ol className="command__gutter" aria-hidden="true">
        {lines.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ol>
      <ol className="command__lines">
        <li className="command__line--prompt">Command:</li>
      </ol>
    </section>
  );
}
