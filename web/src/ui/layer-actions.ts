// What the Layers panel does, as command-line input (DESIGN §5, §15).
//
// The panel has no layer logic: every button is a `LAYER` option with its
// answers, run through the same entry point as typed input, so the log
// shows what was done and undo treats it like anything typed. The layer
// table decides what is allowed and explains any refusal in the log.
import type { LayerRow } from "../cad/engine";

/** The command and the answers to its prompts, in order, ending with Enter. */
export type CommandScript = readonly [command: string, ...answers: string[]];

export const setCurrent = (layer: LayerRow): CommandScript => ["LAYER", "Set", layer.name, ""];

export const toggleOn = (layer: LayerRow): CommandScript => [
  "LAYER",
  layer.on ? "OFF" : "ON",
  layer.name,
  "",
];

export const toggleLocked = (layer: LayerRow): CommandScript => [
  "LAYER",
  layer.locked ? "Unlock" : "Lock",
  layer.name,
  "",
];

export const createLayer = (name: string): CommandScript => ["LAYER", "New", name, ""];

export const renameLayer = (layer: LayerRow, name: string): CommandScript => [
  "LAYER",
  "Rename",
  layer.name,
  name,
  "",
];

export const deleteLayer = (layer: LayerRow): CommandScript => ["LAYER", "Delete", layer.name, ""];

/** The CSS colour of a layer's swatch: its colour, or the theme's foreground. */
export function swatchColor(layer: LayerRow): string {
  return layer.rgb
    ? `rgb(${layer.rgb[0]} ${layer.rgb[1]} ${layer.rgb[2]})`
    : "var(--canvas-entity)";
}

// ── the Properties panel (#141): CHPROP with its answers ──

/** The four properties CHPROP changes, as the panel labels them and as CHPROP's option names them. */
export const CHANGEABLE = [
  { label: "Layer", option: "LAyer" },
  { label: "Color", option: "Color" },
  { label: "Linetype", option: "LType" },
  { label: "Lineweight", option: "LWeight" },
] as const;
export type Changeable = (typeof CHANGEABLE)[number]["label"];

/** CHPROP over the current selection: one property, one value. */
export function changeProperty(property: Changeable, value: string): CommandScript {
  const option = CHANGEABLE.find((entry) => entry.label === property)?.option ?? property;
  return ["CHPROP", option, value, ""];
}

/** The seven colour names and the two indirections: what a colour field offers. */
export const COLOR_CHOICES = [
  "ByLayer",
  "ByBlock",
  "red",
  "yellow",
  "green",
  "cyan",
  "blue",
  "magenta",
  "white",
] as const;

/** The standard lineweights in millimetres, with the three words. */
export const LINEWEIGHT_CHOICES = [
  "ByLayer",
  "ByBlock",
  "Default",
  "0",
  "0.05",
  "0.09",
  "0.13",
  "0.15",
  "0.18",
  "0.2",
  "0.25",
  "0.3",
  "0.35",
  "0.4",
  "0.5",
  "0.53",
  "0.6",
  "0.7",
  "0.8",
  "0.9",
  "1",
  "1.06",
  "1.2",
  "1.4",
  "1.58",
  "2",
  "2.11",
] as const;
