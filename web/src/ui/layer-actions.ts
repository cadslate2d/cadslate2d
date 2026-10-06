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
