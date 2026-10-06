import { describe, expect, test } from "vitest";
import type { LayerRow } from "../cad/engine";
import {
  changeProperty,
  COLOR_CHOICES,
  createLayer,
  deleteLayer,
  LINEWEIGHT_CHOICES,
  renameLayer,
  setCurrent,
  swatchColor,
  toggleLocked,
  toggleOn,
} from "./layer-actions";

const walls: LayerRow = {
  name: "walls",
  color: 1,
  rgb: [255, 0, 0],
  on: true,
  locked: false,
  current: false,
  linetype: "Continuous",
  lineweight: "Default",
};

describe("the Layers panel's actions are LAYER options with their answers", () => {
  test("every action ends with Enter, so the command finishes", () => {
    for (const script of [
      setCurrent(walls),
      toggleOn(walls),
      toggleLocked(walls),
      createLayer("doors"),
      renameLayer(walls, "partitions"),
      deleteLayer(walls),
    ]) {
      expect(script[0]).toBe("LAYER");
      expect(script.at(-1)).toBe("");
    }
  });

  test("switches answer with the opposite of the layer's state", () => {
    expect(toggleOn(walls)).toEqual(["LAYER", "OFF", "walls", ""]);
    expect(toggleOn({ ...walls, on: false })).toEqual(["LAYER", "ON", "walls", ""]);
    expect(toggleLocked(walls)).toEqual(["LAYER", "Lock", "walls", ""]);
    expect(toggleLocked({ ...walls, locked: true })).toEqual(["LAYER", "Unlock", "walls", ""]);
  });

  test("names are passed as typed, spaces and all", () => {
    expect(setCurrent({ ...walls, name: "Ground floor" })).toEqual([
      "LAYER",
      "Set",
      "Ground floor",
      "",
    ]);
    expect(renameLayer(walls, "outer walls")).toEqual([
      "LAYER",
      "Rename",
      "walls",
      "outer walls",
      "",
    ]);
    expect(createLayer("doors")).toEqual(["LAYER", "New", "doors", ""]);
    expect(deleteLayer(walls)).toEqual(["LAYER", "Delete", "walls", ""]);
  });

  test("a swatch is the layer's colour, or the theme's foreground for colour 7", () => {
    expect(swatchColor(walls)).toBe("rgb(255 0 0)");
    expect(swatchColor({ ...walls, color: 7, rgb: undefined })).toBe("var(--canvas-entity)");
  });
});

describe("the Properties panel's changes are CHPROP with its answers", () => {
  test("each property maps to CHPROP's option name and ends with Enter", () => {
    expect(changeProperty("Layer", "walls")).toEqual(["CHPROP", "LAyer", "walls", ""]);
    expect(changeProperty("Color", "red")).toEqual(["CHPROP", "Color", "red", ""]);
    expect(changeProperty("Linetype", "Dashed")).toEqual(["CHPROP", "LType", "Dashed", ""]);
    expect(changeProperty("Lineweight", "0.5")).toEqual(["CHPROP", "LWeight", "0.5", ""]);
  });

  test("the choices offered are the words the commands accept", () => {
    expect(COLOR_CHOICES).toContain("ByLayer");
    expect(COLOR_CHOICES).toHaveLength(9);
    expect(LINEWEIGHT_CHOICES).toHaveLength(27);
    expect(LINEWEIGHT_CHOICES.at(-1)).toBe("2.11");
  });
});
