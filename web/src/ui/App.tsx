import { useEffect, useState } from "react";
import { CadApp } from "../app/cad-app";
import { CadEngine, coreVersion, loadEngine } from "../cad/engine";
import { CanvasView } from "./CanvasView";
import { CommandLine } from "./CommandLine";
import { Header } from "./Header";
import { LayersPanel } from "./LayersPanel";
import { StatusBar } from "./StatusBar";
import { Toolbar } from "./Toolbar";

type CoreState =
  | { status: "loading" }
  | { status: "ready"; app: CadApp; version: string }
  | { status: "failed"; reason: string };

/** Whether a part of the shell is shown, and how to flip it. */
export interface Visibility {
  visible: boolean;
  toggle: () => void;
}

/** Whether the toolbar or a panel is shown is a UI preference: it lives in the browser, not in the drawing. */
function useVisibilityPreference(key: string): Visibility {
  const [visible, setVisible] = useState(() => localStorage.getItem(key) !== "hidden");
  const toggle = () =>
    setVisible((current) => {
      localStorage.setItem(key, current ? "hidden" : "shown");
      return !current;
    });
  return { visible, toggle };
}

export function App() {
  const [core, setCore] = useState<CoreState>({ status: "loading" });
  const toolbar = useVisibilityPreference("cadslate2d.toolbar");
  const layers = useVisibilityPreference("cadslate2d.layers");

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

  // Local-first means the only copy of unexported work is in this tab.
  const app = core.status === "ready" ? core.app : undefined;
  useEffect(() => {
    if (!app) return;
    const guard = (event: BeforeUnloadEvent) => {
      if (app.ui.getSnapshot().unsavedChanges) event.preventDefault();
    };
    window.addEventListener("beforeunload", guard);
    return () => window.removeEventListener("beforeunload", guard);
  }, [app]);

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

  const shell = [
    "shell",
    toolbar.visible && "shell--with-toolbar",
    layers.visible && "shell--with-panel",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={shell}>
      <Header coreVersion={core.version} toolbar={toolbar} layers={layers} />
      {toolbar.visible && <Toolbar app={core.app} />}
      <CanvasView app={core.app} />
      <CommandLine app={core.app} />
      {layers.visible && <LayersPanel app={core.app} />}
      <StatusBar app={core.app} />
    </div>
  );
}
