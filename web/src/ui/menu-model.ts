// The menu bar as data (DESIGN §5, ADR-0034): menus of items, cascades, and
// separators. An item names what it does — a macro over the command line, a
// panel shown or hidden, a dialog — and never does it itself. Whether it is
// greyed or checked is a pure function of what the shell shows.
import type { UiSnapshot } from "../app/ui-store";
import type { Macro } from "../app/macro";

/** A part of the shell a View item shows or hides. */
export type Panel = "toolbar" | "layers" | "properties" | "ai";

/** A dialog of the shell's own that an item opens. */
export type Dialog = "about";

export type MenuAction =
  { readonly macro: Macro } | { readonly panel: Panel } | { readonly dialog: Dialog };

/** What an item's state is decided from. */
export interface MenuContext {
  readonly ui: UiSnapshot;
  readonly panels: Readonly<Record<Panel, boolean>>;
}

export interface MenuItem {
  readonly kind: "item";
  /** The label, `&` before the mnemonic letter: `Save &As...`. */
  readonly label: string;
  /** The key that does the same, as the menu shows it. */
  readonly shortcut?: string;
  readonly action: MenuAction;
  /** Greyed out when this says no; enabled when absent. */
  readonly enabled?: (context: MenuContext) => boolean;
  /** A check mark when this says yes. */
  readonly checked?: (context: MenuContext) => boolean;
}

export interface Cascade {
  readonly kind: "cascade";
  readonly label: string;
  readonly items: readonly MenuEntry[];
}

export interface Separator {
  readonly kind: "separator";
}

export type MenuEntry = MenuItem | Cascade | Separator;

export interface Menu {
  readonly label: string;
  readonly items: readonly MenuEntry[];
}

export const SEPARATOR: Separator = { kind: "separator" };

/** An item that runs `command`, then types `steps`. */
export function command(
  label: string,
  macro: Macro | string,
  options: Pick<MenuItem, "shortcut" | "enabled" | "checked"> = {},
): MenuItem {
  return {
    kind: "item",
    label,
    action: { macro: typeof macro === "string" ? { command: macro } : macro },
    ...options,
  };
}

export function cascade(label: string, items: readonly MenuEntry[]): Cascade {
  return { kind: "cascade", label, items };
}

/** A label's text without its `&`, the mnemonic letter's place in it, and the letter. */
export interface Mnemonic {
  readonly text: string;
  readonly index: number | undefined;
  readonly key: string | undefined;
}

export function mnemonic(label: string): Mnemonic {
  const at = label.indexOf("&");
  if (at < 0 || at === label.length - 1) return { text: label, index: undefined, key: undefined };
  const text = label.slice(0, at) + label.slice(at + 1);
  return { text, index: at, key: text.charAt(at).toLowerCase() };
}

/** Whether an entry can be chosen: an item not greyed out, or a cascade. */
export function selectable(entry: MenuEntry, context: MenuContext): boolean {
  if (entry.kind === "separator") return false;
  if (entry.kind === "cascade") return true;
  return entry.enabled?.(context) ?? true;
}

/** Every item of `menus`, cascades walked, with the labels that lead to it. */
export function* allItems(
  menus: readonly Menu[],
): Generator<{ readonly path: readonly string[]; readonly item: MenuItem }> {
  function* walk(
    entries: readonly MenuEntry[],
    path: readonly string[],
  ): Generator<{ path: readonly string[]; item: MenuItem }> {
    for (const entry of entries) {
      if (entry.kind === "item") yield { path: [...path, mnemonic(entry.label).text], item: entry };
      if (entry.kind === "cascade") yield* walk(entry.items, [...path, mnemonic(entry.label).text]);
    }
  }
  for (const menu of menus) yield* walk(menu.items, [mnemonic(menu.label).text]);
}
