// The local-persistence experiment (DESIGN §25.1; ADR-0019): how long it
// takes to put a drawing's DXF into browser storage on the main thread, by
// IndexedDB and by the OPFS async API, and how much the main thread is
// held up meanwhile. A measurement, not a test: run with `pnpm bench:persistence`.

export type StoreKind = "indexeddb" | "opfs";

export interface StoreMeasurement {
  readonly kind: StoreKind;
  /** The store is not available in this browser: nothing else is measured. */
  readonly unavailable?: string;
  /** Milliseconds from the first call to the write being durable, median of the runs. */
  readonly writeMs: number;
  /** Milliseconds to read the whole text back, median of the runs. */
  readonly readMs: number;
  /** The longest gap between two animation frames during a write, in milliseconds. */
  readonly longestFrameGapMs: number;
}

export interface PersistenceReport {
  readonly bytes: number;
  readonly runs: number;
  readonly stores: StoreMeasurement[];
}

const DB_NAME = "cadslate2d-probe";
const STORE = "drawings";
const KEY = "probe";
const FILE = "probe.dxf";

/** Watches animation frames while `work` runs: the longest gap is how long the thread was held. */
async function withFrameWatch<T>(work: () => Promise<T>): Promise<{ result: T; gap: number }> {
  let last = performance.now();
  let gap = 0;
  let watching = true;
  const tick = () => {
    const now = performance.now();
    gap = Math.max(gap, now - last);
    last = now;
    if (watching) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  const result = await work();
  watching = false;
  return { result, gap };
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? NaN;
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function transact<T>(
  db: IDBDatabase,
  mode: IDBTransactionMode,
  act: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(STORE, mode);
    const request = act(transaction.objectStore(STORE));
    transaction.oncomplete = () => resolve(request.result);
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

async function measureIndexedDb(text: string, runs: number): Promise<StoreMeasurement> {
  if (typeof indexedDB === "undefined") {
    return {
      kind: "indexeddb",
      unavailable: "no indexedDB",
      writeMs: NaN,
      readMs: NaN,
      longestFrameGapMs: NaN,
    };
  }
  const db = await openDatabase();
  const writes: number[] = [];
  const reads: number[] = [];
  let gap = 0;
  for (let run = 0; run < runs; run += 1) {
    const started = performance.now();
    const watched = await withFrameWatch(() =>
      transact(db, "readwrite", (store) => store.put(text, KEY)),
    );
    writes.push(performance.now() - started);
    gap = Math.max(gap, watched.gap);
    const readStarted = performance.now();
    const back = await transact<string>(db, "readonly", (store) => store.get(KEY));
    reads.push(performance.now() - readStarted);
    if (back.length !== text.length) throw new Error("IndexedDB read back a different text");
  }
  db.close();
  return {
    kind: "indexeddb",
    writeMs: median(writes),
    readMs: median(reads),
    longestFrameGapMs: gap,
  };
}

async function measureOpfs(text: string, runs: number): Promise<StoreMeasurement> {
  const unavailable = (why: string): StoreMeasurement => ({
    kind: "opfs",
    unavailable: why,
    writeMs: NaN,
    readMs: NaN,
    longestFrameGapMs: NaN,
  });
  if (!("storage" in navigator) || typeof navigator.storage.getDirectory !== "function") {
    return unavailable("no navigator.storage.getDirectory");
  }
  let root: FileSystemDirectoryHandle;
  try {
    root = await navigator.storage.getDirectory();
  } catch (error) {
    return unavailable(`getDirectory failed: ${String(error)}`);
  }
  const handle = await root.getFileHandle(FILE, { create: true });
  if (typeof handle.createWritable !== "function") {
    return unavailable("no createWritable on the main thread");
  }
  const writes: number[] = [];
  const reads: number[] = [];
  let gap = 0;
  for (let run = 0; run < runs; run += 1) {
    const started = performance.now();
    const watched = await withFrameWatch(async () => {
      const writable = await handle.createWritable();
      await writable.write(text);
      await writable.close();
    });
    writes.push(performance.now() - started);
    gap = Math.max(gap, watched.gap);
    const readStarted = performance.now();
    const back = await (await handle.getFile()).text();
    reads.push(performance.now() - readStarted);
    if (back.length !== text.length) throw new Error("OPFS read back a different text");
  }
  await root.removeEntry(FILE);
  return { kind: "opfs", writeMs: median(writes), readMs: median(reads), longestFrameGapMs: gap };
}

/** Writes `text` to each store `runs` times and reports the medians. */
export async function probePersistence(text: string, runs = 5): Promise<PersistenceReport> {
  const stores: StoreMeasurement[] = [];
  for (const measure of [measureIndexedDb, measureOpfs]) {
    try {
      stores.push(await measure(text, runs));
    } catch (error) {
      stores.push({
        kind: measure === measureIndexedDb ? "indexeddb" : "opfs",
        unavailable: String(error),
        writeMs: NaN,
        readMs: NaN,
        longestFrameGapMs: NaN,
      });
    }
  }
  return { bytes: new TextEncoder().encode(text).length, runs, stores };
}
