// Counters and measurements exposed for end-to-end tests and benchmarks.
// Nothing here can reach CAD state: no entities, no selection, no geometry.
import type { CurveReport } from "../renderer/webgl/curve-probe";
import type { FramesReport } from "../renderer/webgl/frames-probe";
import type { LinetypeReport } from "../renderer/webgl/linetype-probe";
import type { StyleReport } from "../renderer/webgl/style-probe";
import type { TextCandidate, TextReport } from "../renderer/webgl/text-probe";
import type { TransportReport } from "../renderer/webgl/transport-probe";
import type { PointerRests } from "./cad-app";
import type { MainThreadReport } from "./main-thread-probe";
import type { PersistenceReport } from "./persistence-probe";
import type { RendererStats } from "../renderer/webgl/webgl-renderer";

export interface DebugHandle {
  readonly renderer: Readonly<RendererStats>;
  /** How many times the pointer has rested, and where on the canvas (#220): no CAD state. */
  readonly pointerRests: () => PointerRests;
  /** Makes the engine panic (#176), so the recovery path can be tested end to end. */
  readonly crash: () => void;
  /** Runs the render-transport measurement (DESIGN §12.6) on the open drawing. */
  readonly probeTransport: () => Promise<TransportReport>;
  /** Milliseconds to import `text` as DXF into a throwaway session. */
  readonly probeOpen: (text: string) => Promise<number>;
  /** Opens `text` in the application and measures its frames (#171). The drawing on screen is replaced. */
  readonly probeFrames: (text: string, frames?: number) => Promise<FramesReport>;
  /** Runs the curve-rendering experiment (ADR-0009) on `arcCount` generated arcs. */
  readonly probeCurves: (arcCount: number) => Promise<CurveReport>;
  /** Runs the linetype experiment (ADR-0015) on generated dashed paths. */
  readonly probeLinetypes: (segments: number) => Promise<LinetypeReport>;
  /** The linetype experiment's small scene, drawn both ways at three zooms, as PNG data URLs. */
  readonly linetypePictures: () => Promise<{ zoom: number; shader: string; split: string }[]>;
  /** Runs the text rendering experiment (ADR-0016) on generated texts. */
  readonly probeText: (texts: number) => Promise<TextReport>;
  /** The text experiment's one text, drawn each way at three zooms, as PNG data URLs. */
  readonly textPictures: () => Promise<({ zoom: number } & Record<TextCandidate, string>)[]>;
  /** Runs the local-persistence experiment (ADR-0019): `text` written to each browser store. */
  readonly probePersistence: (text: string) => Promise<PersistenceReport>;
  /** Measures what opening, exporting, and autosaving `text` hold the main thread for (#178). */
  readonly probeMainThread: (text: string) => Promise<MainThreadReport>;
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
