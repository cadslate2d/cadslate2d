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
  /** The drawing as DXF, in UTF-8 bytes: what the engine writes in slices (ADR-0024) and reads back. */
  readonly bytes: Uint8Array;
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

/** The look of the shell and the drawing area (DESIGN §7.1: theme is UI state). */
export type Theme = "dark" | "light";

/** What the browser keeps between sessions, for the user (#186). Nothing leaves the browser (§45). */
export interface Settings {
  /** The drafting toggles as last left, as their bits. */
  readonly toggles: number;
  /** The unit a new drawing starts in, as its suffix; empty for unitless. */
  readonly unit: string;
  /** The user's command aliases, one per line as `alias, command`. */
  readonly aliases: string;
  /** How long after the last change the drawing is kept. */
  readonly autosaveDelayMs: number;
  readonly theme: Theme;
}

/** Where the settings are kept: all of them, or nothing yet. */
export interface SettingsStore {
  load(): Partial<Settings>;
  save(settings: Settings): void;
}

export interface HostServices {
  readonly files: FileAccess;
  /** Asks a yes/no question. */
  confirm(message: string): boolean;
  /** Where the autosave is kept. */
  readonly autosave: AutosaveStore;
  /** Where the settings are kept. */
  readonly settings: SettingsStore;
  /** Gives the document its theme; the renderer reads the palette again afterwards. */
  applyTheme(theme: Theme): void;
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
  async load() {
    const stored = await autosaveTransaction<
      (AutosaveRecord & { readonly text?: string }) | undefined
    >("readonly", (store) => store.get(AUTOSAVE_KEY));
    if (!stored) return undefined;
    // A record kept before the snapshot was bytes holds the DXF as text.
    if (stored.bytes === undefined && typeof stored.text === "string") {
      return { ...stored, bytes: new TextEncoder().encode(stored.text) };
    }
    return stored;
  },
  async save(record) {
    await autosaveTransaction("readwrite", (store) => store.put(record, AUTOSAVE_KEY));
  },
  async clear() {
    await autosaveTransaction("readwrite", (store) => store.delete(AUTOSAVE_KEY));
  },
};

let chosenFiles: FileAccess | undefined;

// ── settings: a small JSON record in localStorage ──

const SETTINGS_KEY = "cadslate2d.settings";

/** Reads the kept settings, taking only what is of the right shape; absent or unreadable storage is nothing kept. */
export function parseSettings(text: string | null): Partial<Settings> {
  if (!text) return {};
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return {};
  }
  if (typeof raw !== "object" || raw === null) return {};
  const record = raw as Record<string, unknown>;
  const settings: Record<string, unknown> = {};
  if (typeof record.toggles === "number") settings.toggles = record.toggles;
  if (typeof record.unit === "string") settings.unit = record.unit;
  if (typeof record.aliases === "string") settings.aliases = record.aliases;
  if (typeof record.autosaveDelayMs === "number" && record.autosaveDelayMs >= 0)
    settings.autosaveDelayMs = record.autosaveDelayMs;
  if (record.theme === "dark" || record.theme === "light") settings.theme = record.theme;
  return settings as Partial<Settings>;
}

export const localStorageSettings: SettingsStore = {
  load() {
    try {
      return parseSettings(localStorage.getItem(SETTINGS_KEY));
    } catch {
      return {};
    }
  },
  save(settings) {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Storage that refuses — private mode, a full quota — loses the preference, not the drawing.
    }
  },
};

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
  settings: localStorageSettings,
  applyTheme(theme) {
    document.documentElement.dataset.theme = theme;
  },
  reload: () => window.location.reload(),
};
