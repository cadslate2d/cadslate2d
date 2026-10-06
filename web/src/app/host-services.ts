// Browser facilities the CAD core cannot reach: file pickers, downloads, and
// confirmation prompts. Behind an interface so the app logic can be tested
// without a browser (dependency inversion).

export interface PickedFile {
  readonly name: string;
  readonly bytes: Uint8Array;
}

/** The drawing as last kept in the browser (DESIGN §25; ADR-0019). */
export interface AutosaveRecord {
  /** The file the drawing came from or was exported to, if any. */
  readonly name: string | undefined;
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
  /** Lets the user choose a file. Resolves to `undefined` if they cancel. */
  pickFile(accept: string): Promise<PickedFile | undefined>;
  /** Offers `text` to the user as a file download. */
  saveFile(name: string, text: string, mimeType: string): void;
  /** Asks a yes/no question. */
  confirm(message: string): boolean;
  /** Where the autosave is kept. */
  readonly autosave: AutosaveStore;
}

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
 * main-thread time.
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

/**
 * The universal browser path (DESIGN §25): a plain file input to open and a
 * download to save. It needs no special capability, no server, and no account.
 */
export const browserHost: HostServices = {
  pickFile(accept) {
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

  saveFile(name, text, mimeType) {
    const url = URL.createObjectURL(new Blob([text], { type: mimeType }));
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    link.click();
    URL.revokeObjectURL(url);
  },

  confirm(message) {
    return window.confirm(message);
  },

  autosave: indexedDbAutosave,
};
