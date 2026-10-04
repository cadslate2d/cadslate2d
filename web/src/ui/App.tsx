import { useEffect, useState } from "react";
import { coreVersion, loadEngine } from "../cad/engine";
import { CanvasView } from "./CanvasView";
import { CommandLine } from "./CommandLine";
import { Header } from "./Header";
import { StatusBar } from "./StatusBar";

type CoreState =
  | { status: "loading" }
  | { status: "ready"; version: string }
  | { status: "failed"; reason: string };

export function App() {
  const [core, setCore] = useState<CoreState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    loadEngine().then(
      () => !cancelled && setCore({ status: "ready", version: coreVersion() }),
      (cause: unknown) => !cancelled && setCore({ status: "failed", reason: String(cause) }),
    );
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="shell">
      <Header coreVersion={core.status === "ready" ? core.version : ""} />
      {core.status === "ready" ? (
        <CanvasView />
      ) : (
        <section className="canvas-region" aria-label="Model space">
          <p
            className={`canvas-region__message${core.status === "failed" ? " canvas-region__message--error" : ""}`}
            role={core.status === "failed" ? "alert" : "status"}
          >
            {core.status === "failed" ? `The CAD core failed to load: ${core.reason}` : "Loading…"}
          </p>
        </section>
      )}
      <CommandLine />
      <StatusBar />
    </div>
  );
}
