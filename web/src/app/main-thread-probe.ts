// The main-thread costs a worker could take (DESIGN §35, §37; #178): a
// drawing opened (the DXF parsed and the drawing built), exported (the DXF
// produced), and autosaved (produced, then put into IndexedDB) — each on
// the main thread, in a throwaway engine, with how long the thread was
// held. A measurement, not a test: `pnpm bench:main-thread`.
import { CadEngine, SnapshotStatus } from "../cad/engine";
import { SNAPSHOT_SLICE } from "./cad-app";
import { indexedDbAutosave } from "./host-services";
import { withFrameWatch } from "./persistence-probe";

export interface MainThreadReport {
  /** The import report's first line: what was opened. */
  imported: string;
  /** The DXF the engine writes for it, in bytes. */
  bytes: number;
  /** Opening: parse and build, one synchronous call. */
  openMs: number;
  /** Exporting: the DXF produced, one synchronous call; median of five. */
  exportMs: number;
  /** The autosave as one call: the DXF produced whole, then put into IndexedDB. */
  autosave: Autosave;
  /** The autosave as the app does it (ADR-0024): the DXF in slices a turn of the event loop apart, then put. */
  autosaveSliced: Autosave;
}

export interface Autosave {
  /** From the call to the record being durable. */
  totalMs: number;
  /** The longest gap between two animation frames meanwhile: how long the thread was held. */
  longestFrameGapMs: number;
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? NaN;
}

/** Opens `text` in a throwaway engine and measures what open, export, and autosave hold the main thread for. */
export async function probeMainThread(text: string): Promise<MainThreadReport> {
  const engine = new CadEngine();
  try {
    const bytes = new TextEncoder().encode(text);
    const openStart = performance.now();
    engine.loadDrawing(bytes, "probe.dxf");
    const openMs = performance.now() - openStart;
    const imported = engine.uiState().importReport?.lines[0] ?? "";

    const exports: number[] = [];
    let written = "";
    for (let run = 0; run < 5; run += 1) {
      const start = performance.now();
      written = new TextDecoder().decode(engine.snapshotFile("dxf"));
      exports.push(performance.now() - start);
    }

    const put = (bytes: Uint8Array) =>
      indexedDbAutosave.save({ name: "probe.dxf", bytes, savedAt: Date.now(), exported: true });
    const timed = async (work: () => Promise<void>): Promise<Autosave> => {
      const start = performance.now();
      const watched = await withFrameWatch(work);
      const totalMs = performance.now() - start;
      await indexedDbAutosave.clear();
      return { totalMs, longestFrameGapMs: watched.gap };
    };
    const autosave = await timed(() => put(engine.snapshotFile("dxf")));
    const autosaveSliced = await timed(async () => {
      engine.snapshotBegin();
      while (engine.snapshotStep(SNAPSHOT_SLICE) === SnapshotStatus.Running) {
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
      await put(engine.snapshotBytes());
    });
    return {
      imported,
      bytes: new TextEncoder().encode(written).length,
      openMs,
      exportMs: median(exports),
      autosave,
      autosaveSliced,
    };
  } finally {
    engine.dispose();
  }
}
