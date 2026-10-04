// The drafting toggles of the status bar (DESIGN §5), as data. Rust owns
// their state; this table only says how each is labelled and reached.
import { KeyAction, ToggleBit } from "../cad/engine";

export type ToggleId = "osnap" | "grid" | "ortho" | "polar";

export interface ToggleSpec {
  readonly id: ToggleId;
  readonly label: string;
  readonly key: string;
  readonly bit: number;
  readonly action: KeyAction;
  /** What the toggle does, for its tooltip. */
  readonly hint: string;
}

/** Toggles the session knows about, in status-bar order. */
const NOT_YET = "Its drafting behaviour is not available yet.";

export const TOGGLES: readonly ToggleSpec[] = [
  {
    id: "grid",
    label: "GRID",
    key: "F7",
    bit: ToggleBit.Grid,
    action: KeyAction.ToggleGrid,
    hint: NOT_YET,
  },
  {
    id: "ortho",
    label: "ORTHO",
    key: "F8",
    bit: ToggleBit.Ortho,
    action: KeyAction.ToggleOrtho,
    hint: NOT_YET,
  },
  {
    id: "polar",
    label: "POLAR",
    key: "F10",
    bit: ToggleBit.Polar,
    action: KeyAction.TogglePolar,
    hint: NOT_YET,
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
export const UNAVAILABLE_TOGGLES = ["SNAP", "OTRACK", "LWT"] as const;

export function toggleSpec(id: ToggleId): ToggleSpec {
  const spec = TOGGLES.find((toggle) => toggle.id === id);
  if (!spec) throw new Error(`unknown toggle ${id}`);
  return spec;
}
