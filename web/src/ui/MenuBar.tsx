import { AI_SERVICE_URL } from "../app/ai-service";
import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import type { CadApp } from "../app/cad-app";
import { classifyMenuKey, menuShortcut } from "../app/key-routing";
import { macroText } from "../app/macro";
import {
  mnemonic,
  selectable,
  type Menu,
  type MenuAction,
  type MenuContext,
  type MenuEntry,
  type Panel,
} from "./menu-model";
import { entriesAt, openedByKeyboard, stepMenu, type OpenState } from "./menu-nav";

export interface MenuBarProps {
  readonly app: CadApp;
  readonly menus: readonly Menu[];
  readonly panels: Readonly<Record<Panel, boolean>>;
  /** Carries out what an item names. The bar holds no logic of its own (ADR-0034). */
  readonly onAction: (action: MenuAction) => void;
}

/** A label with its mnemonic letter underlined, when the keyboard shows them. */
function Label({ label, underline }: { label: string; underline: boolean }) {
  const { text, index } = mnemonic(label);
  if (!underline || index === undefined) return <>{text}</>;
  return (
    <>
      {text.slice(0, index)}
      <span className="menubar__mnemonic">{text.charAt(index)}</span>
      {text.slice(index + 1)}
    </>
  );
}

/** The entries of one open list, and the cascade open beside it. */
export function MenuList({
  entries,
  level,
  state,
  context,
  underline,
  onHover,
  onChoose,
  labelledBy,
}: {
  entries: readonly MenuEntry[];
  level: number;
  state: OpenState;
  context: MenuContext;
  underline: boolean;
  onHover: (level: number, index: number) => void;
  onChoose: (entry: MenuEntry) => void;
  labelledBy?: string;
}): ReactNode {
  const highlighted = state.path[level];
  return (
    <div
      className={`menubar__list${level > 0 ? " menubar__list--cascade" : ""}`}
      role="menu"
      aria-labelledby={labelledBy}
      // Keep keyboard focus on the command line.
      onMouseDown={(event) => event.preventDefault()}
    >
      {entries.map((entry, index) => {
        if (entry.kind === "separator") {
          return <div key={index} className="menubar__separator" role="separator" />;
        }
        const enabled = selectable(entry, context);
        const active = highlighted === index;
        const expanded = entry.kind === "cascade" && active && state.depth > level + 1;
        const checked = entry.kind === "item" ? entry.checked?.(context) : undefined;
        const className = [
          "menubar__entry",
          active && "menubar__entry--active",
          !enabled && "menubar__entry--disabled",
        ]
          .filter(Boolean)
          .join(" ");
        return (
          <div
            key={index}
            className={className}
            role={checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-label={mnemonic(entry.label).text}
            aria-keyshortcuts={entry.kind === "item" ? entry.shortcut : undefined}
            aria-checked={checked}
            aria-disabled={!enabled || undefined}
            aria-haspopup={entry.kind === "cascade" ? "menu" : undefined}
            aria-expanded={entry.kind === "cascade" ? expanded : undefined}
            title={
              entry.kind === "item" && "macro" in entry.action
                ? macroText(entry.action.macro)
                : undefined
            }
            onMouseEnter={() => onHover(level, index)}
            onClick={() => enabled && onChoose(entry)}
          >
            <span className="menubar__check" aria-hidden="true">
              {checked ? "✓" : ""}
            </span>
            <span className="menubar__label">
              <Label label={entry.label} underline={underline} />
            </span>
            <span className="menubar__shortcut">
              {entry.kind === "item" ? (entry.shortcut ?? "") : "▸"}
            </span>
            {expanded && entry.kind === "cascade" && (
              <MenuList
                entries={entry.items}
                level={level + 1}
                state={state}
                context={context}
                underline={underline}
                onHover={onHover}
                onChoose={onChoose}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

const quickIcon = (path: ReactNode) => (
  <svg
    viewBox="0 0 24 24"
    width="16"
    height="16"
    aria-hidden="true"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    {path}
  </svg>
);

/** The quick actions at the row's right, as the visual identity draws them. */
const QUICK: readonly { label: string; command: string; icon: ReactNode }[] = [
  { label: "New", command: "NEW", icon: quickIcon(<path d="M6 3h8l4 4v14H6zM14 3v4h4" />) },
  { label: "Open", command: "OPEN", icon: quickIcon(<path d="M3 7h7l2 2h9v10H3z" />) },
  {
    label: "Save",
    command: "SAVE",
    icon: quickIcon(<path d="M5 3h11l3 3v15H5zM8 3v5h7V3M8 21v-7h8v7" />),
  },
  {
    label: "Export",
    command: "EXPORT",
    icon: quickIcon(<path d="M12 15V3M8 7l4-4 4 4M5 13v8h14v-8" />),
  },
];

/**
 * The classic menu bar (DESIGN §5, ADR-0034): File, Edit, View, …, each a
 * list of items that run commands as typed. It owns only which list is open;
 * focus stays on the command line.
 */
export function MenuBar({ app, menus, panels, onAction }: MenuBarProps) {
  const ui = useSyncExternalStore(app.ui.subscribe, app.ui.getSnapshot);
  const [open, setOpen] = useState<OpenState | undefined>(undefined);
  const bar = useRef<HTMLElement>(null);
  const context: MenuContext = { ui, panels };

  // The keyboard listener is installed once; it reads the latest of these.
  const latest = useRef({ open, context, onAction });
  useEffect(() => {
    latest.current = { open, context, onAction };
  });

  const close = () => setOpen(undefined);
  const openMenu = (menu: number) => setOpen({ menu, path: [], depth: 1, keyboard: false });

  const choose = (entry: MenuEntry) => {
    if (entry.kind !== "item") return;
    close();
    onAction(entry.action);
  };

  const hover = (level: number, index: number) =>
    setOpen((state) => {
      if (!state) return state;
      const entries = entriesAt(menus[state.menu]?.items ?? [], state.path, level);
      const entry = entries[index];
      const path = [...state.path.slice(0, level), index];
      // Hovering a cascade opens it, as a Windows menu does.
      const depth = entry?.kind === "cascade" ? level + 2 : level + 1;
      return { ...state, path, depth, keyboard: false };
    });

  // While a menu is open, a click outside it closes it.
  useEffect(() => {
    if (!open) return;
    // `pointerdown`: the canvas cancels it, which keeps `mousedown` from following.
    const onPointer = (event: PointerEvent) => {
      if (!bar.current?.contains(event.target as Node)) close();
    };
    window.addEventListener("pointerdown", onPointer, true);
    return () => window.removeEventListener("pointerdown", onPointer, true);
  }, [open]);

  // The keyboard (#248), as in AutoCAD's classic menus: Alt alone puts the
  // bar in reach with its letters underlined, Alt and a letter opens that
  // menu, and while the menus have the keyboard every key is theirs — the
  // command line hears none of it.
  useEffect(() => {
    /** Alt went down and nothing else has since: its release reaches the bar. */
    let altAlone = false;
    const take = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const { open: state, context: now, onAction: act } = latest.current;
      if (event.key === "Alt") {
        altAlone = !event.ctrlKey && !event.metaKey && !event.shiftKey;
        if (state) take(event);
        return;
      }
      altAlone = false;
      const letter = menuShortcut(event);
      if (letter !== undefined) {
        const menu = menus.findIndex((each) => mnemonic(each.label).key === letter);
        if (menu < 0) return;
        take(event);
        setOpen(openedByKeyboard(menu, menus, now));
        return;
      }
      if (!state) return;
      const key = classifyMenuKey(event);
      if (key.kind === "pass") return;
      take(event);
      const { next, chosen } = stepMenu(state, key, menus, now);
      setOpen(next);
      if (chosen) act(chosen.action);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key !== "Alt" || !altAlone) return;
      altAlone = false;
      // The browser's own menu bar answers Alt too; this page's comes first.
      event.preventDefault();
      setOpen((state) => (state ? undefined : { menu: 0, path: [], depth: 0, keyboard: true }));
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [menus]);

  return (
    <nav className="menubar" aria-label="Menu bar" ref={bar}>
      <div className="menubar__menus" role="menubar" aria-label="Menus">
        {menus.map((menu, index) => {
          const id = `menubar-${index}`;
          const highlighted = open?.menu === index;
          const expanded = highlighted && open.depth > 0;
          return (
            <div key={menu.label} className="menubar__menu">
              <button
                id={id}
                type="button"
                className={`menubar__title${highlighted ? " menubar__title--open" : ""}`}
                role="menuitem"
                aria-haspopup="menu"
                aria-expanded={expanded}
                // Keep keyboard focus on the command line.
                onMouseDown={(event) => {
                  event.preventDefault();
                  if (expanded) close();
                  else openMenu(index);
                }}
                onMouseEnter={() => open && !expanded && openMenu(index)}
              >
                <Label label={menu.label} underline={open?.keyboard ?? false} />
              </button>
              {expanded && open && (
                <MenuList
                  entries={menu.items}
                  level={0}
                  state={open}
                  context={context}
                  underline={open.keyboard}
                  onHover={hover}
                  onChoose={choose}
                  labelledBy={id}
                />
              )}
            </div>
          );
        })}
      </div>
      <span className="menubar__spacer" />
      <div className="menubar__quick" role="group" aria-label="Quick actions">
        {AI_SERVICE_URL && (
          <button
            type="button"
            className="menubar__quick-button menubar__quick-button--ai"
            aria-pressed={panels.ai}
            title="Show or hide the AI Assistant"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onAction({ panel: "ai" })}
          >
            {quickIcon(
              <path d="M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM18 15l.9 2.1L21 18l-2.1.9L18 21l-.9-2.1L15 18l2.1-.9z" />,
            )}
            <span>AI Assistant</span>
          </button>
        )}
        {QUICK.map((quick) => (
          <button
            key={quick.command}
            type="button"
            className="menubar__quick-button"
            title={quick.command}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onAction({ macro: { command: quick.command } })}
          >
            {quick.icon}
            <span>{quick.label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}
