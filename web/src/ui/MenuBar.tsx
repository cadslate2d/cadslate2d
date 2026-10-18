import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import type { CadApp } from "../app/cad-app";
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

/** Which menu is open, and how deep: the highlighted entry at each level, and how many levels show. */
interface OpenState {
  readonly menu: number;
  /** The highlighted entry of each open list, outermost first; the last may be none. */
  readonly path: readonly (number | undefined)[];
  /** Lists shown: 1 for the menu alone, 2 with a cascade beside it, … */
  readonly depth: number;
}

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
function MenuList({
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

  const close = () => setOpen(undefined);
  const openMenu = (menu: number) => setOpen({ menu, path: [], depth: 1 });

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
      return { ...state, path, depth };
    });

  // While a menu is open, a click outside it closes it, and Esc closes one level.
  useEffect(() => {
    if (!open) return;
    // `pointerdown`: the canvas cancels it, which keeps `mousedown` from following.
    const onPointer = (event: PointerEvent) => {
      if (!bar.current?.contains(event.target as Node)) close();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // The menu's, not the command line's: nothing is cancelled.
      event.preventDefault();
      event.stopImmediatePropagation();
      setOpen((state) =>
        state && state.depth > 1
          ? { ...state, depth: state.depth - 1, path: state.path.slice(0, state.depth - 1) }
          : undefined,
      );
    };
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  return (
    <nav className="menubar" aria-label="Menu bar" ref={bar}>
      <div className="menubar__menus" role="menubar" aria-label="Menus">
        {menus.map((menu, index) => {
          const id = `menubar-${index}`;
          const expanded = open?.menu === index;
          return (
            <div key={menu.label} className="menubar__menu">
              <button
                id={id}
                type="button"
                className={`menubar__title${expanded ? " menubar__title--open" : ""}`}
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
                <Label label={menu.label} underline={false} />
              </button>
              {expanded && open && (
                <MenuList
                  entries={menu.items}
                  level={0}
                  state={open}
                  context={context}
                  underline={false}
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

/** The entries of the list open at `level`: the menu's own, or a cascade's along `path`. */
export function entriesAt(
  items: readonly MenuEntry[],
  path: readonly (number | undefined)[],
  level: number,
): readonly MenuEntry[] {
  let entries = items;
  for (let at = 0; at < level; at += 1) {
    const index = path[at];
    const entry = index === undefined ? undefined : entries[index];
    if (entry?.kind !== "cascade") return [];
    entries = entry.items;
  }
  return entries;
}
