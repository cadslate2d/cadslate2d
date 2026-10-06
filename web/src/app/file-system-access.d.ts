// The File System Access API (DESIGN §25.3), as far as the app uses it. The
// DOM library knows the handles and the writable stream but not the pickers
// or the permission queries, which only Chromium-based browsers have.

interface FilePickerAcceptType {
  readonly description?: string;
  readonly accept: Record<string, readonly string[]>;
}

interface FilePickerOptions {
  readonly types?: readonly FilePickerAcceptType[];
  readonly excludeAcceptAllOption?: boolean;
}

interface OpenFilePickerOptions extends FilePickerOptions {
  readonly multiple?: boolean;
}

interface SaveFilePickerOptions extends FilePickerOptions {
  readonly suggestedName?: string;
}

interface FileSystemHandlePermissionDescriptor {
  readonly mode?: "read" | "readwrite";
}

interface FileSystemHandle {
  queryPermission?(descriptor?: FileSystemHandlePermissionDescriptor): Promise<PermissionState>;
  requestPermission?(descriptor?: FileSystemHandlePermissionDescriptor): Promise<PermissionState>;
}

interface Window {
  showOpenFilePicker?(options?: OpenFilePickerOptions): Promise<FileSystemFileHandle[]>;
  showSaveFilePicker?(options?: SaveFilePickerOptions): Promise<FileSystemFileHandle>;
}
