// Resolves the drawing-area palette from theme tokens (CSS custom properties).
// Rust refers to geometry by semantic role; the theme decides what each role
// looks like, so theme remains UI state (DESIGN §7.1).
import { StyleRole } from "../cad/engine";

/** Straight (non-premultiplied) RGBA, each component in 0..1. */
export type Rgba = readonly [number, number, number, number];

export interface StrokeStyle {
  readonly color: Rgba;
  /** Lineweight in CSS pixels. */
  readonly width: number;
}

export interface CanvasPalette {
  readonly background: Rgba;
  readonly strokes: Readonly<Record<StyleRole, StrokeStyle>>;
}

const ROLE_TOKENS: Readonly<Record<StyleRole, string>> = {
  [StyleRole.Entity]: "entity",
  [StyleRole.Selection]: "selection",
  [StyleRole.Preview]: "preview",
  [StyleRole.Crosshair]: "crosshair",
  [StyleRole.SelectionWindow]: "selection-window",
  [StyleRole.SelectionCrossing]: "selection-crossing",
  [StyleRole.SnapMarker]: "snap-marker",
  [StyleRole.Tracking]: "tracking",
  [StyleRole.GridMinor]: "grid-minor",
  [StyleRole.GridMajor]: "grid-major",
  [StyleRole.Grip]: "grip",
  [StyleRole.GripHot]: "grip-hot",
};

/** Parses `#rgb`, `#rrggbb`, or `#rrggbbaa`. */
export function parseHexColor(text: string): Rgba {
  const hex = text.trim().replace(/^#/, "");
  const expanded = hex.length === 3 ? [...hex].map((digit) => digit + digit).join("") : hex;
  if (!/^([0-9a-f]{6}|[0-9a-f]{8})$/i.test(expanded)) {
    throw new Error(`unsupported colour token "${text}": expected a hex colour`);
  }
  const channel = (index: number) => parseInt(expanded.slice(index * 2, index * 2 + 2), 16) / 255;
  return [channel(0), channel(1), channel(2), expanded.length === 8 ? channel(3) : 1];
}

/** Reads a token value; `read` is `getPropertyValue` of a computed style. */
export function readPalette(read: (property: string) => string): CanvasPalette {
  const stroke = (name: string): StrokeStyle => {
    const width = Number(read(`--canvas-lineweight-${name}`));
    if (!(width > 0)) throw new Error(`missing or invalid --canvas-lineweight-${name}`);
    return { color: parseHexColor(read(`--canvas-${name}`)), width };
  };
  const strokes = Object.fromEntries(
    Object.entries(ROLE_TOKENS).map(([role, name]) => [role, stroke(name)]),
  ) as Record<StyleRole, StrokeStyle>;
  return { background: parseHexColor(read("--canvas-background")), strokes };
}

/** The palette defined by the document's current theme. */
export function documentPalette(): CanvasPalette {
  const style = getComputedStyle(document.documentElement);
  return readPalette((property) => style.getPropertyValue(property));
}
