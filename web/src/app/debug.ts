// Numeric counters exposed for end-to-end tests. Nothing here can reach CAD
// state: no entities, no selection, no geometry.
import type { RendererStats } from "../renderer/webgl/webgl-renderer";

export interface DebugHandle {
  readonly renderer: Readonly<RendererStats>;
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
