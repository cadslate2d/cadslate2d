import { useEffect, useState } from "react";
import { CadApp } from "../app/cad-app";
import { CadEngine, coreVersion, loadEngine } from "../cad/engine";
import { CanvasView } from "./CanvasView";
import { CommandLine } from "./CommandLine";
import { Header } from "./Header";
import { StatusBar } from "./StatusBar";

type CoreState =
  | { status: "loading" }
  | { status: "ready"; app: CadApp; version: string }
  | { status: "failed"; reason: string };

export function App() {
  const [core, setCore] = useState<CoreState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    let app: CadApp | undefined;
    loadEngine().then(
      () => {
        if (cancelled) return;
        app = new CadApp(new CadEngine());
        setCore({ status: "ready", app, version: coreVersion() });
      },
      (cause: unknown) => !cancelled && setCore({ status: "failed", reason: String(cause) }),
    );
    return () => {
      cancelled = true;
      app?.dispose();
    };
  }, []);

  if (core.status !== "ready") {
    const failed = core.status === "failed";
    return (
      <div className="shell">
        <Header coreVersion="" />
        <section className="canvas-region" aria-label="Model space">
          <p
            className={`canvas-region__message${failed ? " canvas-region__message--error" : ""}`}
            role={failed ? "alert" : "status"}
          >
            {failed ? `The CAD core failed to load: ${core.reason}` : "Loading…"}
          </p>
        </section>
      </div>
    );
  }

  return (
    <div className="shell">
      <Header coreVersion={core.version} />
      <CanvasView app={core.app} />
      <CommandLine app={core.app} />
      <StatusBar app={core.app} />
    </div>
  );
}
