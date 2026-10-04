import { useEffect, useState } from "react";
import { CadApp } from "../app/cad-app";
import { CadEngine, coreVersion, loadEngine } from "../cad/engine";
import { CanvasView } from "./CanvasView";
import { CommandLine } from "./CommandLine";
import { Header } from "./Header";
import { StatusBar } from "./StatusBar";
import { Toolbar } from "./Toolbar";

type CoreState =
  | { status: "loading" }
  | { status: "ready"; app: CadApp; version: string }
  | { status: "failed"; reason: string };

const TOOLBAR_KEY = "cadslate2d.toolbar";

/** Toolbar visibility is a UI preference: it lives in the browser, not in the drawing. */
function useToolbarPreference(): [boolean, () => void] {
  const [visible, setVisible] = useState(() => localStorage.getItem(TOOLBAR_KEY) !== "hidden");
  const toggle = () =>
    setVisible((current) => {
      localStorage.setItem(TOOLBAR_KEY, current ? "hidden" : "shown");
      return !current;
    });
  return [visible, toggle];
}

export function App() {
  const [core, setCore] = useState<CoreState>({ status: "loading" });
  const [toolbarVisible, toggleToolbar] = useToolbarPreference();

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

  return (
    <div className={`shell${toolbarVisible ? " shell--with-toolbar" : ""}`}>
      <Header
        coreVersion={core.version}
        toolbar={{ visible: toolbarVisible, toggle: toggleToolbar }}
      />
      {toolbarVisible && <Toolbar app={core.app} />}
      <CanvasView app={core.app} />
      <CommandLine app={core.app} />
      <StatusBar app={core.app} />
    </div>
  );
}
