import { useEffect, useState, useSyncExternalStore } from "react";
import { CadApp } from "../app/cad-app";
import { CadEngine, coreVersion, loadEngine } from "../cad/engine";
import { CanvasView } from "./CanvasView";
import { CommandLine } from "./CommandLine";
import { ExportWarning } from "./ExportWarning";
import { Failure } from "./Failure";
import { Header } from "./Header";
import { ImportReport } from "./ImportReport";
import { Recovery } from "./Recovery";
import { LayersPanel } from "./LayersPanel";
import { PropertiesPanel } from "./PropertiesPanel";
import { StatusBar } from "./StatusBar";
import { TextEditor } from "./TextEditor";
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
  const properties = useVisibilityPreference("cadslate2d.properties");

  useEffect(() => {
    let cancelled = false;
    let app: CadApp | undefined;
    loadEngine().then(
      () => {
        if (cancelled) return;
        app = new CadApp(new CadEngine());
        setCore({ status: "ready", app, version: coreVersion() });
        // A drawing kept by a tab that closed before exporting is offered back.
        void app.start();
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
  const ui = useSyncExternalStore(
    app?.ui.subscribe ?? (() => () => {}),
    app?.ui.getSnapshot ?? (() => undefined),
  );
  useEffect(() => {
    const name = ui?.documentName;
    const marker = ui?.unsavedChanges ? "• " : "";
    document.title = name ? `${marker}${name} — CadSlate2D` : `${marker}CadSlate2D`;
  }, [ui?.documentName, ui?.unsavedChanges]);
  useEffect(() => {
    if (!app) return;
    const guard = (event: BeforeUnloadEvent) => {
      // After the engine has stopped, leaving loses nothing more: the
      // reload is the way back.
      const { unsavedChanges, failure } = app.ui.getSnapshot();
      if (unsavedChanges && !failure) event.preventDefault();
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

  const panels = layers.visible || properties.visible;
  const shell = ["shell", toolbar.visible && "shell--with-toolbar", panels && "shell--with-panels"]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={shell}>
      <Header
        coreVersion={core.version}
        toolbar={toolbar}
        layers={layers}
        properties={properties}
      />
      {toolbar.visible && <Toolbar app={core.app} />}
      <CanvasView app={core.app} />
      <ImportReport app={core.app} />
      <ExportWarning app={core.app} />
      <Recovery app={core.app} />
      <Failure app={core.app} />
      <TextEditor app={core.app} />
      <CommandLine app={core.app} />
      {panels && (
        <div className="panels">
          {layers.visible && <LayersPanel app={core.app} />}
          {properties.visible && <PropertiesPanel app={core.app} />}
        </div>
      )}
      <StatusBar app={core.app} />
    </div>
  );
}
