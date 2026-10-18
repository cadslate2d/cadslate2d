// Moving through an open menu from the keyboard (#248): pure functions over
// the model, as a Windows menu moves — separators and greyed items are
// passed over, and the ends wrap.
import type { MenuKey } from "../app/key-routing";
import {
  mnemonic,
  selectable,
  type Menu,
  type MenuContext,
  type MenuEntry,
  type MenuItem,
} from "./menu-model";

/**
 * The next entry that can be chosen from `from`, `step` +1 down or −1 up,
 * wrapping; from none, the first (down) or the last (up). None when no
 * entry can be chosen.
 */
export function nextSelectable(
  entries: readonly MenuEntry[],
  from: number | undefined,
  step: 1 | -1,
  context: MenuContext,
): number | undefined {
  const count = entries.length;
  if (count === 0) return undefined;
  let at = from ?? (step === 1 ? -1 : count);
  for (let tried = 0; tried < count; tried += 1) {
    at = (((at + step) % count) + count) % count;
    const entry = entries[at];
    if (entry && selectable(entry, context)) return at;
  }
  return undefined;
}

/** The first entry that can be chosen (Home), or the last (End). */
export function endSelectable(
  entries: readonly MenuEntry[],
  end: "first" | "last",
  context: MenuContext,
): number | undefined {
  return nextSelectable(entries, undefined, end === "first" ? 1 : -1, context);
}

/** The entry whose mnemonic is `key`, if it can be chosen. */
export function byMnemonic(
  entries: readonly MenuEntry[],
  key: string,
  context: MenuContext,
): number | undefined {
  const wanted = key.toLowerCase();
  const index = entries.findIndex(
    (entry) => entry.kind !== "separator" && mnemonic(entry.label).key === wanted,
  );
  const entry = entries[index];
  return entry && selectable(entry, context) ? index : undefined;
}

/** Which menu is open, and how deep: the highlighted entry at each level, and how many levels show. */
export interface OpenState {
  readonly menu: number;
  /** The highlighted entry of each open list, outermost first; the last may be none. */
  readonly path: readonly (number | undefined)[];
  /** Lists shown: 0 for the title alone, highlighted from the keyboard; 1 for the menu; 2 with a cascade beside it; … */
  readonly depth: number;
  /** Opened or moved by the keyboard: the mnemonics are underlined. */
  readonly keyboard: boolean;
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

/** Menu `menu` opened from the keyboard, its first (or last) entry highlighted. */
export function openedByKeyboard(
  menu: number,
  menus: readonly Menu[],
  context: MenuContext,
  end: "first" | "last" = "first",
): OpenState {
  const items = menus[menu]?.items ?? [];
  return { menu, path: [endSelectable(items, end, context)], depth: 1, keyboard: true };
}

/** What a key leaves: the menus as they are next, and the item to run, if one was chosen. */
export interface MenuStep {
  readonly next: OpenState | undefined;
  readonly chosen?: MenuItem;
}

/**
 * One key in the menus (#248), as in a Windows menu:
 * - with only a title highlighted, Left and Right move along the bar, and
 *   Down, Enter, or its letter opens a menu;
 * - in a menu, Up and Down move, Home and End go to the ends, Right opens
 *   a cascade or the next menu, and Left closes a cascade or opens the
 *   previous menu;
 * - Enter or a letter runs an item or opens a cascade;
 * - Esc closes one level, down to the title when the keyboard opened the
 *   menu; Tab leaves.
 */
export function stepMenu(
  state: OpenState,
  key: MenuKey,
  menus: readonly Menu[],
  context: MenuContext,
): MenuStep {
  const count = menus.length;
  const wrap = (index: number) => (((index % count) + count) % count) as number;
  if (key.kind === "leave") return { next: undefined };
  if (state.depth === 0) {
    switch (key.kind) {
      case "left":
      case "right":
        return { next: { ...state, menu: wrap(state.menu + (key.kind === "right" ? 1 : -1)) } };
      case "down":
      case "choose":
        return { next: openedByKeyboard(state.menu, menus, context) };
      case "up":
        return { next: openedByKeyboard(state.menu, menus, context, "last") };
      case "close":
        return { next: undefined };
      case "mnemonic": {
        const menu = menus.findIndex((each) => mnemonic(each.label).key === key.key);
        return { next: menu < 0 ? state : openedByKeyboard(menu, menus, context) };
      }
      default:
        return { next: state };
    }
  }
  const level = state.depth - 1;
  const entries = entriesAt(menus[state.menu]?.items ?? [], state.path, level);
  const at = state.path[level];
  const highlight = (index: number | undefined): MenuStep => ({
    next: { ...state, path: [...state.path.slice(0, level), index], keyboard: true },
  });
  const enter = (index: number): MenuStep => {
    const entry = entries[index];
    if (!entry || !selectable(entry, context)) return { next: state };
    if (entry.kind === "item") return { next: undefined, chosen: entry };
    if (entry.kind !== "cascade") return { next: state };
    return {
      next: {
        ...state,
        path: [...state.path.slice(0, level), index, endSelectable(entry.items, "first", context)],
        depth: state.depth + 1,
        keyboard: true,
      },
    };
  };
  const back: MenuStep = {
    next: {
      ...state,
      depth: state.depth - 1,
      path: state.path.slice(0, state.depth - 1),
      keyboard: true,
    },
  };
  switch (key.kind) {
    case "up":
    case "down":
      return highlight(nextSelectable(entries, at, key.kind === "down" ? 1 : -1, context));
    case "home":
    case "end":
      return highlight(endSelectable(entries, key.kind === "home" ? "first" : "last", context));
    case "right":
      if (at !== undefined && entries[at]?.kind === "cascade") return enter(at);
      return { next: openedByKeyboard(wrap(state.menu + 1), menus, context) };
    case "left":
      if (state.depth > 1) return back;
      return { next: openedByKeyboard(wrap(state.menu - 1), menus, context) };
    case "choose":
      return at === undefined ? { next: state } : enter(at);
    case "mnemonic": {
      const index = byMnemonic(entries, key.key, context);
      return index === undefined ? { next: state } : enter(index);
    }
    case "close":
      if (state.depth > 1) return back;
      return { next: state.keyboard ? { ...state, depth: 0, path: [] } : undefined };
    default:
      return { next: state };
  }
}
