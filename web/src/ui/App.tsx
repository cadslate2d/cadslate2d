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
import { About } from "./About";
import { MenuBar } from "./MenuBar";
import type { MenuAction, Panel } from "./menu-model";
import { MENUS } from "./menus";
import { PropertiesPanel } from "./PropertiesPanel";
import { AiPanel } from "./AiPanel";
import { AiAssistant, browserSessionStore } from "../app/ai-assistant";
import { httpAiClient } from "../app/ai-client";
import { AI_SERVICE_URL } from "../app/ai-service";
import { StatusBar } from "./StatusBar";
import { TextEditor } from "./TextEditor";
import { SettingsPanel } from "./SettingsPanel";
import { Toolbar } from "./Toolbar";

type CoreState =
  | { status: "loading" }
  | { status: "ready"; app: CadApp; assistant: AiAssistant; version: string }
  | { status: "failed"; reason: string };

/** A sign-in link opens the app with its token in the fragment. */
const SIGNING_IN = typeof window !== "undefined" && window.location.hash.includes("ai-sign-in=");

/** The assistant over the app, talking to this build's service, if it has one (ADR-0037). */
function assistantFor(app: CadApp): AiAssistant {
  return new AiAssistant(
    {
      aiContext: () => app.engine.aiContext(),
      unit: () => app.engine.uiState().unit,
      propose: (intent) => app.propose(intent),
      acceptProposal: () => app.acceptProposal(),
      rejectProposal: () => app.rejectProposal(),
      proposalSummary: () => app.engine.proposalSummary(),
      // Beside the app, so the drawing in this tab is not left (ADR-0002).
      openPage: (url) => void window.open(url, "_blank", "noopener"),
    },
    AI_SERVICE_URL ? httpAiClient(AI_SERVICE_URL) : undefined,
    browserSessionStore,
  );
}

/** Whether a part of the shell is shown, and how to flip it. */
export interface Visibility {
  visible: boolean;
  toggle: () => void;
}

/** Whether the toolbar or a panel is shown is a UI preference: it lives in the browser, not in the drawing. */
function useVisibilityPreference(key: string, shownAtFirst = true): Visibility {
  const [visible, setVisible] = useState(() => {
    const kept = localStorage.getItem(key);
    return kept === null ? shownAtFirst : kept !== "hidden";
  });
  const toggle = () =>
    setVisible((current) => {
      localStorage.setItem(key, current ? "hidden" : "shown");
      return !current;
    });
  return { visible, toggle };
}

export function App() {
  const [core, setCore] = useState<CoreState>({ status: "loading" });
  const [about, setAbout] = useState(false);
  const toolbar = useVisibilityPreference("cadslate2d.toolbar");
  const layers = useVisibilityPreference("cadslate2d.layers");
  const properties = useVisibilityPreference("cadslate2d.properties");
  // Off until the user opens it (ADR-0037), or comes back from a sign-in link.
  // The AI panel is open from the start in a build that has the service.
  const ai = useVisibilityPreference("cadslate2d.ai", SIGNING_IN || AI_SERVICE_URL !== undefined);

  useEffect(() => {
    let cancelled = false;
    let app: CadApp | undefined;
    // Back from Checkout or the billing portal, the plan may have changed.
    let comeBack: (() => void) | undefined;
    loadEngine().then(
      () => {
        if (cancelled) return;
        app = new CadApp(new CadEngine());
        const assistant = assistantFor(app);
        setCore({ status: "ready", app, assistant, version: coreVersion() });
        // A drawing kept by a tab that closed before exporting is offered back.
        void app.start();
        comeBack = () => void assistant.refresh();
        window.addEventListener("focus", comeBack);
        // A sign-in link's token is spent, and taken off the address bar.
        void assistant.start(window.location.hash).then((rest) => {
          if (rest !== window.location.hash) {
            history.replaceState(
              null,
              "",
              `${window.location.pathname}${window.location.search}${rest}`,
            );
          }
        });
      },
      (cause: unknown) => !cancelled && setCore({ status: "failed", reason: String(cause) }),
    );
    return () => {
      cancelled = true;
      if (comeBack) window.removeEventListener("focus", comeBack);
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

  const visibility: Record<Panel, Visibility> = { toolbar, layers, properties, ai };
  // What a menu item names, carried out (ADR-0034): a macro typed into the
  // command line, or a part of the shell shown or hidden.
  const act = (action: MenuAction) => {
    if ("macro" in action) core.app.runMacro(action.macro);
    else if ("panel" in action) visibility[action.panel].toggle();
    else if (action.dialog === "about") setAbout(true);
  };
  const panels = layers.visible || properties.visible || ai.visible;
  const shell = ["shell", toolbar.visible && "shell--with-toolbar", panels && "shell--with-panels"]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={shell}>
      <Header coreVersion={core.version} />
      <MenuBar
        app={core.app}
        menus={MENUS}
        panels={{
          toolbar: toolbar.visible,
          layers: layers.visible,
          properties: properties.visible,
          ai: ai.visible,
        }}
        onAction={act}
      />
      {toolbar.visible && <Toolbar app={core.app} />}
      <CanvasView app={core.app} />
      <ImportReport app={core.app} />
      <ExportWarning app={core.app} />
      <Recovery app={core.app} />
      <Failure app={core.app} />
      {about && <About version={core.version} onClose={() => setAbout(false)} />}
      <TextEditor app={core.app} />
      <SettingsPanel app={core.app} />
      <CommandLine app={core.app} />
      {panels && (
        <div className="panels">
          {layers.visible && <LayersPanel app={core.app} />}
          {properties.visible && <PropertiesPanel app={core.app} />}
          {ai.visible && (
            <AiPanel
              assistant={core.assistant}
              privacyUrl={AI_SERVICE_URL ? new URL("privacy", AI_SERVICE_URL).href : undefined}
              pricingUrl={AI_SERVICE_URL ? new URL("pricing", AI_SERVICE_URL).href : undefined}
            />
          )}
        </div>
      )}
      <StatusBar app={core.app} />
    </div>
  );
}
