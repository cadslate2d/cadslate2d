import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import type { CadApp } from "../app/cad-app";
import { MenuList } from "./MenuBar";
import type { MenuAction, MenuEntry } from "./menu-model";
import type { OpenState } from "./menu-nav";
import { shortcutMenu } from "./shortcut-menu";

const NO_PANELS = { toolbar: false, layers: false, properties: false, ai: false };

/**
 * The drawing area's right-click menu (AutoCAD's shortcut menu): opened by
 * a right click on the canvas, where the pointer is, with what the session
 * is doing deciding what it offers. The click itself is the engine's — at
 * the idle prompt it picks the object under the pointer — and the menu
 * reads the selection after it. Escape, a click elsewhere, or a choice
 * closes it.
 */
export function ShortcutMenu({
  app,
  onAction,
}: {
  app: CadApp;
  onAction: (action: MenuAction) => void;
}) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const [at, setAt] = useState<{ x: number; y: number }>();
  const [state, setState] = useState<OpenState>({ menu: 0, path: [], depth: 1, keyboard: false });
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const open = (event: MouseEvent) => {
      if (!(event.target instanceof HTMLCanvasElement)) return;
      if (!event.target.closest(".canvas-region")) return;
      event.preventDefault();
      setState({ menu: 0, path: [], depth: 1, keyboard: false });
      setAt({ x: event.clientX, y: event.clientY });
    };
    window.addEventListener("contextmenu", open);
    return () => window.removeEventListener("contextmenu", open);
  }, []);

  useEffect(() => {
    if (!at) return;
    const close = () => setAt(undefined);
    const outside = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) close();
    };
    // Escape closes the menu and goes no further: it is not the command's.
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      close();
    };
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    window.addEventListener("wheel", close, { passive: true });
    return () => {
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("wheel", close);
    };
  }, [at]);

  // Kept inside the window: opened near an edge, it opens the other way.
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!at || !menu) return;
    const { width, height } = menu.getBoundingClientRect();
    const x = Math.max(0, Math.min(at.x, window.innerWidth - width));
    const y = at.y + height > window.innerHeight ? Math.max(0, at.y - height) : at.y;
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
  });

  if (!at) return null;
  const entries = shortcutMenu(ui);
  const hover = (level: number, index: number) => {
    const list = level === 0 ? entries : cascadeAt(entries, state.path[0]);
    const entry = list[index];
    setState({
      ...state,
      path: [...state.path.slice(0, level), index],
      depth: entry?.kind === "cascade" ? level + 2 : level + 1,
    });
  };
  const choose = (entry: MenuEntry) => {
    if (entry.kind !== "item") return;
    setAt(undefined);
    onAction(entry.action);
  };
  return (
    <div ref={ref} className="shortcut-menu" style={{ left: at.x, top: at.y }}>
      <MenuList
        entries={entries}
        level={0}
        state={state}
        context={{ ui, panels: NO_PANELS }}
        underline={false}
        onHover={hover}
        onChoose={choose}
      />
    </div>
  );
}

function cascadeAt(entries: readonly MenuEntry[], index: number | undefined) {
  const entry = index === undefined ? undefined : entries[index];
  return entry?.kind === "cascade" ? entry.items : [];
}
