// How wide the docked panels' column is, and how its height is shared
// among the panels shown: dragged by the column's left edge and by the
// dividers between panels, or moved from the keyboard, and remembered by
// the browser. Layout only: no drawing data.
import {
  useCallback,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from "react";

/** The panels that share the column, top to bottom. */
export type DockedPanel = "layers" | "properties" | "ai";

/** Each panel's share of the column's height. */
export type Shares = Record<DockedPanel, number>;

/** The shares the panels start with: the conversation needs room. */
export const DEFAULT_SHARES: Shares = { layers: 1, properties: 1, ai: 3 };

/** The narrowest the column goes, and the widest, as a part of the window. */
export const MIN_WIDTH = 280;
export const MAX_WIDTH_PART = 0.7;
/** The least height a panel keeps: its title and a line. */
export const MIN_HEIGHT = 72;
/** How far an arrow key moves a handle. */
export const STEP = 16;

const KEY = "cadslate2d.panels";

interface Sizes {
  /** The column's width in pixels; undefined for the theme's. */
  readonly width?: number;
  readonly shares: Shares;
}

function load(): Sizes {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<Sizes> | null;
    const shares = { ...DEFAULT_SHARES };
    for (const panel of Object.keys(DEFAULT_SHARES) as DockedPanel[]) {
      const share = kept?.shares?.[panel];
      if (typeof share === "number" && Number.isFinite(share) && share > 0) shares[panel] = share;
    }
    const width =
      typeof kept?.width === "number" && Number.isFinite(kept.width) && kept.width >= MIN_WIDTH
        ? kept.width
        : undefined;
    return { width, shares };
  } catch {
    return { shares: { ...DEFAULT_SHARES } };
  }
}

function save(sizes: Sizes) {
  try {
    localStorage.setItem(KEY, JSON.stringify(sizes));
  } catch {
    // A browser that keeps nothing keeps the sizes for this visit alone.
  }
}

/** The widest the column may be in a window `windowWidth` wide. */
export const maxWidth = (windowWidth: number) =>
  Math.max(MIN_WIDTH, Math.floor(windowWidth * MAX_WIDTH_PART));

/** `width` kept between the narrowest and the widest the window allows. */
export const clampWidth = (width: number, windowWidth: number) =>
  Math.round(Math.min(Math.max(width, MIN_WIDTH), maxWidth(windowWidth)));

/**
 * The heights of two neighbouring panels, `above` and `below`, after their
 * divider moves `by` pixels down: what one gains the other gives up, and
 * neither goes under `MIN_HEIGHT` (unless they were already).
 */
export function moveDivider(above: number, below: number, by: number): [number, number] {
  const total = above + below;
  const least = Math.min(MIN_HEIGHT, total / 2);
  const next = Math.min(Math.max(above + by, least), total - least);
  return [next, total - next];
}

export function usePanelSizes() {
  const [sizes, setSizes] = useState<Sizes>(load);
  const update = useCallback((next: Sizes) => {
    setSizes(next);
    save(next);
  }, []);
  return { sizes, update };
}

/** The column's width and the panels' shares, as the shell's custom properties. */
export function sizeStyle(sizes: Sizes): CSSProperties {
  return {
    ...(sizes.width !== undefined ? { "--size-panel": `${sizes.width}px` } : {}),
    "--share-layers": sizes.shares.layers,
    "--share-properties": sizes.shares.properties,
    "--share-ai": sizes.shares.ai,
  } as CSSProperties;
}

/** The handle on the column's left edge: drag it, or use the arrow keys. */
export function WidthHandle({ sizes, update }: { sizes: Sizes; update: (sizes: Sizes) => void }) {
  // The column's width: the panels and the margin on their right, which is
  // what `--size-panel` sizes. The panels' box alone is that margin short,
  // and a drag begun from it jumped by the margin.
  const current = (handle: HTMLElement) => {
    const column = handle.parentElement;
    if (!column) return sizes.width ?? MIN_WIDTH;
    const margin = parseFloat(getComputedStyle(column).marginRight) || 0;
    return column.getBoundingClientRect().width + margin;
  };
  const to = (width: number) => update({ ...sizes, width: clampWidth(width, window.innerWidth) });
  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const [x, width] = [event.clientX, current(handle)];
    const move = (moved: globalThis.PointerEvent) => to(width + x - moved.clientX);
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  };
  const key = (event: KeyboardEvent<HTMLDivElement>) => {
    const width = current(event.currentTarget);
    const next = {
      ArrowLeft: width + STEP,
      ArrowRight: width - STEP,
      Home: maxWidth(window.innerWidth),
      End: MIN_WIDTH,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    to(next);
  };
  return (
    <div
      className="panels__edge"
      role="separator"
      aria-orientation="vertical"
      aria-label="Panel width"
      aria-valuemin={MIN_WIDTH}
      aria-valuemax={maxWidth(window.innerWidth)}
      aria-valuenow={sizes.width}
      title="Drag to resize the panels; double-click for the usual width"
      tabIndex={0}
      onPointerDown={down}
      onKeyDown={key}
      onDoubleClick={() => update({ ...sizes, width: undefined })}
    />
  );
}

/** The divider between two panels shown one above the other. */
export function Divider({
  above,
  below,
  sizes,
  update,
}: {
  above: DockedPanel;
  below: DockedPanel;
  sizes: Sizes;
  update: (sizes: Sizes) => void;
}) {
  const names: Record<DockedPanel, string> = {
    layers: "Layers",
    properties: "Properties",
    ai: "AI Assistant",
  };
  // The panels' heights now, read from the page, each as its outer height
  // and its borders: a panel's share is the height inside its borders,
  // which is what flex shares out, so the two neighbours trade exactly
  // what the pointer moved and the others do not move at all.
  const heights = (divider: HTMLElement) => {
    const column = divider.parentElement;
    const outer = { ...sizes.shares };
    const borders = { layers: 0, properties: 0, ai: 0 };
    const shown: DockedPanel[] = [];
    for (const panel of Object.keys(outer) as DockedPanel[]) {
      const element = column?.querySelector<HTMLElement>(`:scope > [data-dock="${panel}"]`);
      if (!element) continue;
      const style = getComputedStyle(element);
      borders[panel] =
        (parseFloat(style.borderTopWidth) || 0) + (parseFloat(style.borderBottomWidth) || 0);
      outer[panel] = element.getBoundingClientRect().height;
      shown.push(panel);
    }
    // A hidden panel keeps its part of the height, in the same measure as
    // the panels shown, so it comes back the size it had.
    const unit =
      shown.reduce((sum, panel) => sum + outer[panel] - borders[panel], 0) /
      shown.reduce((sum, panel) => sum + sizes.shares[panel], 0);
    for (const panel of Object.keys(outer) as DockedPanel[]) {
      if (!shown.includes(panel)) outer[panel] = sizes.shares[panel] * unit;
    }
    return { outer, borders };
  };
  const by = ({ outer, borders }: ReturnType<typeof heights>, pixels: number) => {
    // The least height is the panel's outer one, as it is seen.
    const [a, b] = moveDivider(outer[above], outer[below], pixels);
    const moved = { ...outer, [above]: a, [below]: b };
    const shares = { ...sizes.shares };
    for (const panel of Object.keys(shares) as DockedPanel[]) {
      shares[panel] = Math.max(moved[panel] - borders[panel], 1);
    }
    update({ ...sizes, shares });
  };
  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const divider = event.currentTarget;
    divider.setPointerCapture(event.pointerId);
    const [y, start] = [event.clientY, heights(divider)];
    const move = (moved: globalThis.PointerEvent) => by(start, moved.clientY - y);
    const up = () => {
      divider.removeEventListener("pointermove", move);
      divider.removeEventListener("pointerup", up);
      divider.removeEventListener("pointercancel", up);
    };
    divider.addEventListener("pointermove", move);
    divider.addEventListener("pointerup", up);
    divider.addEventListener("pointercancel", up);
  };
  const key = (event: KeyboardEvent<HTMLDivElement>) => {
    const pixels = {
      ArrowUp: -STEP,
      ArrowDown: STEP,
      Home: -Infinity,
      End: Infinity,
    }[event.key];
    if (pixels === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    by(heights(event.currentTarget), pixels);
  };
  return (
    <div
      className="panels__divider"
      role="separator"
      aria-orientation="horizontal"
      aria-label={`Height of ${names[above]} and ${names[below]}`}
      title="Drag to share the height; double-click for the usual shares"
      tabIndex={0}
      onPointerDown={down}
      onKeyDown={key}
      onDoubleClick={() => update({ ...sizes, shares: { ...DEFAULT_SHARES } })}
    />
  );
}
