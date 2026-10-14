// The drafting toggles of the status bar (DESIGN §5), as data. Rust owns
// their state; this table only says how each is labelled and reached.
import { KeyAction, ToggleBit } from "../cad/engine";

export type ToggleId = "snap" | "osnap" | "grid" | "ortho" | "polar" | "lineweight";

export interface ToggleSpec {
  readonly id: ToggleId;
  readonly label: string;
  /** The function key that flips it, if it has one. */
  readonly key?: string;
  readonly bit: number;
  readonly action: KeyAction;
  /** What the toggle does, for its tooltip. */
  readonly hint: string;
}

/** Toggles the session knows about, in status-bar order. */
export const TOGGLES: readonly ToggleSpec[] = [
  {
    id: "snap",
    label: "SNAP",
    key: "F9",
    bit: ToggleBit.Snap,
    action: KeyAction.ToggleSnap,
    hint: "The pointer lands on the snap grid. Type SNAP to set its spacing.",
  },
  {
    id: "grid",
    label: "GRID",
    key: "F7",
    bit: ToggleBit.Grid,
    action: KeyAction.ToggleGrid,
    hint: "Shows a reference grid under the drawing. Type GRID to set its spacing.",
  },
  {
    id: "ortho",
    label: "ORTHO",
    key: "F8",
    bit: ToggleBit.Ortho,
    action: KeyAction.ToggleOrtho,
    hint: "The pointer draws horizontally or vertically from the previous point. Switches POLAR off.",
  },
  {
    id: "polar",
    label: "POLAR",
    key: "F10",
    bit: ToggleBit.Polar,
    action: KeyAction.TogglePolar,
    hint: "The pointer locks onto angles at a fixed increment from the previous point. Type POLARANG to set it. Switches ORTHO off.",
  },
  {
    id: "osnap",
    label: "OSNAP",
    key: "F3",
    bit: ToggleBit.Osnap,
    action: KeyAction.ToggleOsnap,
    hint: "Clicks land exactly on endpoints, midpoints, centres, quadrants, and intersections. Type OSNAP to choose which.",
  },
];

/** Shown in the reference design but not implemented: visibly unavailable. */
export const UNAVAILABLE_TOGGLES = ["OTRACK"] as const;

/** Lineweight display. It has no function key: it is flipped from the status bar. */
export const LINEWEIGHT_TOGGLE: ToggleSpec = {
  id: "lineweight",
  label: "LWT",
  bit: ToggleBit.Lineweight,
  action: KeyAction.ToggleLineweight,
  hint: "Draws lines as heavy as their lineweights say. Off, every line is drawn at the ordinary width. Type LWEIGHT to set the lineweight of new objects.",
};

/**
 * The status bar, left to right, as in the reference design: the toggles
 * that work, and the names of those that do not yet.
 */
export const STATUS_BAR: readonly (ToggleSpec | string)[] = [
  ...TOGGLES,
  ...UNAVAILABLE_TOGGLES,
  LINEWEIGHT_TOGGLE,
];

export function toggleSpec(id: ToggleId): ToggleSpec {
  const spec = [...TOGGLES, LINEWEIGHT_TOGGLE].find((toggle) => toggle.id === id);
  if (!spec) throw new Error(`unknown toggle ${id}`);
  return spec;
}
