// Browser facilities the CAD core cannot reach: files, confirmation prompts,
// and the autosave slot. Behind an interface so the app logic can be tested
// without a browser (dependency inversion).

/** A file the browser can find again: its name, and its handle when the browser gives one. */
export interface FileTarget {
  readonly name: string;
  /** The File System Access handle, where the browser has the API (DESIGN §25.3). */
  readonly handle?: FileSystemFileHandle;
}

export interface PickedFile {
  readonly name: string;
  readonly bytes: Uint8Array;
  /** Where the file is, when it can be written back to. */
  readonly target?: FileTarget;
}

/**
 * How files are opened and saved. Two implementations: the universal one
 * — a file input to open, a download to save — and the one for browsers
 * with the File System Access API, which write back to the file they opened.
 * The app chooses by capability, never by user agent.
 */
export interface FileAccess {
  /** Whether a saved file can be written again without asking where. */
  readonly writesBack: boolean;
  /** Lets the user choose a file. Resolves to `undefined` if they cancel. */
  open(accept: string): Promise<PickedFile | undefined>;
  /**
   * Saves `text` to a file the user chooses — a dialog, or a download where
   * there is none — and says where it went; `undefined` if they cancelled.
   */
  saveAs(suggestedName: string, text: string, mimeType: string): Promise<FileTarget | undefined>;
  /** Writes `text` back to `target`. `false` when the browser would not, so the caller asks where instead. */
  write(target: FileTarget, text: string): Promise<boolean>;
}

/** The drawing as last kept in the browser (DESIGN §25; ADR-0019). */
export interface AutosaveRecord {
  /** The file the drawing came from or was saved to, if any. */
  readonly name: string | undefined;
  /** Its handle, where the browser gave one, so a recovered drawing knows its file. */
  readonly handle?: FileSystemFileHandle;
  /** The drawing as DXF. */
  readonly text: string;
  /** When it was kept, in milliseconds since the epoch. */
  readonly savedAt: number;
  /** Whether a file holds this state too: then there is nothing to recover. */
  readonly exported: boolean;
}

/** One slot of browser storage for the autosave. */
export interface AutosaveStore {
  load(): Promise<AutosaveRecord | undefined>;
  save(record: AutosaveRecord): Promise<void>;
  clear(): Promise<void>;
}

export interface HostServices {
  readonly files: FileAccess;
  /** Asks a yes/no question. */
  confirm(message: string): boolean;
  /** Where the autosave is kept. */
  readonly autosave: AutosaveStore;
  /** Loads the page afresh: a new engine, and the kept drawing offered back. */
  reload(): void;
}

// ── files: the universal path ──

/**
 * The universal browser path (DESIGN §25): a plain file input to open and a
 * download to save. It needs no special capability, no server, and no account.
 */
export const downloadFiles: FileAccess = {
  writesBack: false,

  open(accept) {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.accept = accept;
      input.addEventListener("change", () => {
        const file = input.files?.[0];
        if (!file) {
          resolve(undefined);
          return;
        }
        void file
          .arrayBuffer()
          .then((buffer) => resolve({ name: file.name, bytes: new Uint8Array(buffer) }));
      });
      input.addEventListener("cancel", () => resolve(undefined));
      input.click();
    });
  },

  saveAs(suggestedName, text, mimeType) {
    const url = URL.createObjectURL(new Blob([text], { type: mimeType }));
    const link = document.createElement("a");
    link.href = url;
    link.download = suggestedName;
    link.click();
    URL.revokeObjectURL(url);
    return Promise.resolve({ name: suggestedName });
  },

  write() {
    return Promise.resolve(false);
  },
};

// ── files: the File System Access path ──

/** The picker's description of the files it shows. */
const DXF_TYPES = [{ description: "DXF drawing", accept: { "application/dxf": [".dxf"] } }];

/** A cancelled picker rejects with an AbortError: that is an answer, not a failure. */
function cancelled(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

/** Whether the handle may be written, asking the user if the browser wants that. */
async function mayWrite(handle: FileSystemFileHandle): Promise<boolean> {
  const options = { mode: "readwrite" } as const;
  if ((await handle.queryPermission?.(options)) === "granted") return true;
  return (await handle.requestPermission?.(options)) === "granted";
}

async function writeThrough(handle: FileSystemFileHandle, text: string): Promise<void> {
  const writable = await handle.createWritable();
  await writable.write(text);
  await writable.close();
}

/**
 * Where the browser has `showOpenFilePicker` and `showSaveFilePicker`
 * (DESIGN §25.3): the opened file is kept as a handle and written back to.
 */
export const fileSystemAccessFiles: FileAccess = {
  writesBack: true,

  async open() {
    const picker = window.showOpenFilePicker;
    if (!picker) return undefined;
    try {
      const [handle] = await picker.call(window, { types: DXF_TYPES, multiple: false });
      if (!handle) return undefined;
      const file = await handle.getFile();
      return {
        name: file.name,
        bytes: new Uint8Array(await file.arrayBuffer()),
        target: { name: file.name, handle },
      };
    } catch (error) {
      if (cancelled(error)) return undefined;
      throw error;
    }
  },

  async saveAs(suggestedName, text) {
    const picker = window.showSaveFilePicker;
    if (!picker) return undefined;
    try {
      const handle = await picker.call(window, { suggestedName, types: DXF_TYPES });
      await writeThrough(handle, text);
      return { name: handle.name, handle };
    } catch (error) {
      if (cancelled(error)) return undefined;
      throw error;
    }
  },

  async write(target, text) {
    if (!target.handle || !(await mayWrite(target.handle))) return false;
    await writeThrough(target.handle, text);
    return true;
  },
};

/** The path this browser offers: write-back where the API is, downloads elsewhere. */
export function fileAccessForThisBrowser(): FileAccess {
  return typeof window !== "undefined" &&
    typeof window.showSaveFilePicker === "function" &&
    typeof window.showOpenFilePicker === "function"
    ? fileSystemAccessFiles
    : downloadFiles;
}

// ── the autosave slot ──

const DB_NAME = "cadslate2d";
const AUTOSAVE_STORE = "autosave";
const AUTOSAVE_KEY = "current";

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(AUTOSAVE_STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/** One transaction on the autosave store, resolved when it is durable. */
async function autosaveTransaction<T>(
  mode: IDBTransactionMode,
  act: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await openDatabase();
  try {
    return await new Promise<T>((resolve, reject) => {
      const transaction = db.transaction(AUTOSAVE_STORE, mode);
      const request = act(transaction.objectStore(AUTOSAVE_STORE));
      transaction.oncomplete = () => resolve(request.result);
      transaction.onerror = () => reject(transaction.error);
      transaction.onabort = () => reject(transaction.error);
    });
  } finally {
    db.close();
  }
}

/**
 * The autosave in IndexedDB (ADR-0019): one record under one key, written
 * whole. Available on every browser's main thread, which the OPFS writable
 * stream is not; a 16 MB drawing is written in well under a frame's worth of
 * main-thread time. A file handle in the record survives the reload with it.
 */
export const indexedDbAutosave: AutosaveStore = {
  load() {
    return autosaveTransaction<AutosaveRecord | undefined>("readonly", (store) =>
      store.get(AUTOSAVE_KEY),
    );
  },
  async save(record) {
    await autosaveTransaction("readwrite", (store) => store.put(record, AUTOSAVE_KEY));
  },
  async clear() {
    await autosaveTransaction("readwrite", (store) => store.delete(AUTOSAVE_KEY));
  },
};

let chosenFiles: FileAccess | undefined;

export const browserHost: HostServices = {
  // Chosen on first use, when there is a window to ask.
  get files() {
    chosenFiles ??= fileAccessForThisBrowser();
    return chosenFiles;
  },

  confirm(message) {
    return window.confirm(message);
  },

  autosave: indexedDbAutosave,
  reload: () => window.location.reload(),
};
