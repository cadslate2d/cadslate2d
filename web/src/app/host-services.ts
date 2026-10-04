// Browser facilities the CAD core cannot reach: file pickers, downloads, and
// confirmation prompts. Behind an interface so the app logic can be tested
// without a browser (dependency inversion).

export interface PickedFile {
  readonly name: string;
  readonly bytes: Uint8Array;
}

export interface HostServices {
  /** Lets the user choose a file. Resolves to `undefined` if they cancel. */
  pickFile(accept: string): Promise<PickedFile | undefined>;
  /** Offers `text` to the user as a file download. */
  saveFile(name: string, text: string, mimeType: string): void;
  /** Asks a yes/no question. */
  confirm(message: string): boolean;
}

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
};
