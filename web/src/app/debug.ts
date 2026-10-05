// Counters and measurements exposed for end-to-end tests and benchmarks.
// Nothing here can reach CAD state: no entities, no selection, no geometry.
import type { CurveReport } from "../renderer/webgl/curve-probe";
import type { StyleReport } from "../renderer/webgl/style-probe";
import type { TransportReport } from "../renderer/webgl/transport-probe";
import type { RendererStats } from "../renderer/webgl/webgl-renderer";

export interface DebugHandle {
  readonly renderer: Readonly<RendererStats>;
  /** Runs the render-transport measurement (DESIGN §12.6) on the open drawing. */
  readonly probeTransport: () => Promise<TransportReport>;
  /** Milliseconds to import `text` as DXF into a throwaway session. */
  readonly probeOpen: (text: string) => Promise<number>;
  /** Runs the curve-rendering experiment (ADR-0009) on `arcCount` generated arcs. */
  readonly probeCurves: (arcCount: number) => Promise<CurveReport>;
  /** Runs the colour-transport experiment (ADR-0014) on generated segments. */
  readonly probeStyles: (
    segments: number,
    styles: number,
    order: "grouped" | "interleaved",
  ) => Promise<StyleReport>;
}

declare global {
  interface Window {
    __cadslate?: DebugHandle;
  }
}

/** Counters are published only in dev builds or when the URL carries `?debug`. */
export function debugEnabled(): boolean {
  return import.meta.env.DEV || new URLSearchParams(window.location.search).has("debug");
}

export function publishDebugHandle(handle: DebugHandle | undefined): void {
  if (handle === undefined) {
    delete window.__cadslate;
  } else if (debugEnabled()) {
    window.__cadslate = handle;
  }
}
