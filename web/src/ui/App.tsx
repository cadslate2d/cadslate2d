import { useEffect, useState } from "react";
import { coreVersion, loadEngine } from "../cad/engine";

export function App() {
  const [version, setVersion] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    void loadEngine().then(() => {
      if (!cancelled) setVersion(coreVersion());
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main>
      <h1>CadSlate2D</h1>
      <p data-testid="core-version">{version ? `core ${version}` : "loading core…"}</p>
    </main>
  );
}
